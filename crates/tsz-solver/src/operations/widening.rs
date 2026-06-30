//! Type widening operations for literal types.
//!
//! This module implements TypeScript's type widening rules, where literal types
//! are widened to their primitive types in certain contexts for usability.
//!
//! ## Widening Rules
//!
//! - **String literals** → `string`
//! - **Number literals** → `number`
//! - **Boolean literals** → `boolean`
//! - **`BigInt` literals** → `bigint`
//! - **Union types**: All members are widened recursively
//! - **Object types**: Property types are widened unless `readonly`
//! - **Type parameters**: Never widened
//! - **Unique symbols**: Never widened

use crate::diagnostics::display_provenance::{
    self, AliasApplicationPriority, AliasApplicationProvenance,
    FreshObjectLiteralDisplayProvenance, UnionOriginProvenance,
};
use crate::types::{ObjectFlags, TypeData, TypeId};
use rustc_hash::FxHashSet;

mod annotation;

pub use self::annotation::{
    AnnotationLiteralWideningPolicy, AnnotationWideningOutcome,
    widen_annotation_literals_for_display, widen_annotation_literals_for_display_resolved,
};

/// Propagate `display_alias` from the original type to the widened type.
///
/// When a type produced by evaluating a generic Application (e.g., `Record<string, 1>`)
/// is widened (e.g., to `Record<string, number>`-shaped object), the new TypeId loses
/// its `display_alias` mapping. This function copies the mapping forward so the formatter
/// can still show the alias name instead of the expanded structural form.
#[inline]
fn propagate_display_alias(
    db: &dyn crate::construction::TypeDatabase,
    original: TypeId,
    widened: TypeId,
) {
    if original != widened
        && let Some(alias) = display_provenance::display_alias(db, original)
    {
        display_provenance::record_alias_application(
            db,
            AliasApplicationProvenance {
                evaluated: widened,
                application: alias,
            },
            AliasApplicationPriority::PreserveExisting,
        );
    }
}

fn replay_widen_type_display_provenance(
    db: &dyn crate::construction::TypeDatabase,
    original: TypeId,
    widened: TypeId,
) {
    if original == widened {
        return;
    }
    if let Some(display_props) = db.get_display_properties(original) {
        display_provenance::record_fresh_object_literal_display(
            db,
            FreshObjectLiteralDisplayProvenance {
                type_id: widened,
                properties: display_props.as_ref().clone(),
            },
        );
    }
    propagate_display_alias(db, original, widened);
}

fn readonly_property_preserves_top_level_type(
    db: &dyn crate::construction::TypeDatabase,
    type_id: TypeId,
) -> bool {
    match db.lookup(type_id) {
        Some(TypeData::Literal(_) | TypeData::UniqueSymbol(_)) => true,
        // A union whose members are *all* themselves preservable top-level
        // literals/unit types (`-1 | 0 | 1`, `"a" | "b"`) is preserved as a
        // whole: tsc's `getWidenedType` never widens a regular literal union on
        // a `readonly`/non-widening property. This mirrors the member-by-member
        // union distribution in the main widen path, where literal members are
        // already kept — without it, a non-widening property carrying a literal
        // union (e.g. `comparison: x as -1 | 0 | 1`) would widen to its base
        // primitive (`number`) and produce false TS2322/TS2345.
        Some(TypeData::Union(list_id)) => {
            let members = db.type_list(list_id);
            !members.is_empty()
                && members
                    .iter()
                    .all(|&member| readonly_property_preserves_top_level_type(db, member))
        }
        _ => type_id == TypeId::BOOLEAN_TRUE || type_id == TypeId::BOOLEAN_FALSE,
    }
}

/// Public API to widen a literal type to its primitive.
///
/// This is the main entry point for type widening in the checker.
///
/// ## Example
///
/// ```text
/// use crate::operations::widening::widen_type;
///
/// // Widen a literal string to the string primitive
/// let widened = widen_type(db, string_literal_type);
/// assert_eq!(widened, TypeId::STRING);
/// ```
pub fn widen_type(db: &dyn crate::construction::TypeDatabase, type_id: TypeId) -> TypeId {
    // Fast path: most intrinsic types are already widened and never change.
    // Boolean literals (true/false) are intrinsic but DO need widening to boolean.
    if type_id.is_intrinsic()
        && type_id != crate::types::TypeId::BOOLEAN_TRUE
        && type_id != crate::types::TypeId::BOOLEAN_FALSE
    {
        return type_id;
    }
    // Fast path: non-literal, non-union, non-intersection types don't widen.
    // Object, Function, Callable, Array, Tuple, TypeParameter, etc. are stable.
    if matches!(
        db.lookup(type_id),
        Some(
            crate::types::TypeData::Function(_)
                | crate::types::TypeData::Callable(_)
                | crate::types::TypeData::TypeParameter(_)
                | crate::types::TypeData::Enum(_, _)
                | crate::types::TypeData::Mapped(_)
                | crate::types::TypeData::Conditional(_)
                | crate::types::TypeData::Application(_)
                | crate::types::TypeData::Lazy(_)
                | crate::types::TypeData::IndexAccess(_, _)
                | crate::types::TypeData::KeyOf(_)
                | crate::types::TypeData::TemplateLiteral(_)
                | crate::types::TypeData::ThisType
                | crate::types::TypeData::Error
        )
    ) {
        return type_id;
    }
    // Note: literals, unions, intersections, objects, arrays, and tuples may
    // still contain widenable data, so they must go through the full path.
    //
    // Widening is a pure function of the immutable interned type, but the call
    // below allocates a fresh per-call recursion guard, so a wide union widened
    // once per reference recomputes the full O(N) member walk every time —
    // O(N^2) over an N-arm discriminated-union switch (#13598). Memoize the
    // canonical-flag result root-wide so repeats collapse to O(1). Only this
    // entry uses the memo; `widen_type_deep`/display variants compute different
    // results for the same `TypeId` (e.g. they descend function parameters) and
    // must not read or write it.
    if let Some(widened) = db.widen_type_memo(type_id) {
        replay_widen_type_display_provenance(db, type_id, widened);
        return widened;
    }
    use rustc_hash::FxHashMap;
    let mut cache = FxHashMap::default();
    let result = widen_type_cached(db, type_id, &mut cache, true, true, false, false, false);
    db.set_widen_type_memo(type_id, result);
    result
}

/// Widen for diagnostic display: like `widen_type` but preserves boolean
/// literal intrinsics (`true`/`false`) so that narrowed types like
/// `string | false` display correctly instead of `string | boolean`.
///
/// Does NOT recurse into function/callable parameter types, preserving
/// literal parameters so `(x: "bar") => number` displays as-is rather than
/// being widened to `(x: string) => number`.
pub fn widen_type_for_display(
    db: &dyn crate::construction::TypeDatabase,
    type_id: TypeId,
) -> TypeId {
    use rustc_hash::FxHashMap;
    let mut cache = FxHashMap::default();
    widen_type_cached(db, type_id, &mut cache, false, false, false, false, false)
}

/// Widen for diagnostic display while preserving the literal property types of
/// *non-fresh* objects, mirroring tsc's `getWidenedType` (which only widens
/// types carrying the widening flag — `FRESH_LITERAL` here).
///
/// Like `widen_type_for_display`, but a declared/computed object such as
/// `{ kind: "other" }` — or any member produced by distributing a conditional
/// over a union — keeps its literal members instead of being widened to
/// `{ kind: string }`. A *fresh* object literal still widens, matching tsc's
/// diagnostics for fresh sources.
///
/// This is the correct widener for rendering an already-typed source/member in
/// assignability diagnostics. The plain `widen_type_for_display` keeps widening
/// non-fresh objects because some callers compare its result for display-equal
/// suppression and rely on that legacy shape.
pub fn widen_type_for_display_preserving_non_fresh(
    db: &dyn crate::construction::TypeDatabase,
    type_id: TypeId,
) -> TypeId {
    use rustc_hash::FxHashMap;
    let mut cache = FxHashMap::default();
    widen_type_cached(db, type_id, &mut cache, false, false, false, false, true)
}

/// Widen for call-argument diagnostic display: widens boolean literal
/// intrinsics inside compound shapes so a tuple argument like
/// `[1, 2, false, true]` renders as `[number, number, boolean, boolean]`,
/// matching tsc's TS2345 source-type display. Like `widen_type_for_display`,
/// it does NOT recurse into function/callable parameter types so contravariant
/// surfaces are preserved.
///
/// Tuples that carry a rest element keep their boolean literal elements
/// unchanged (`[string, number, true, ...(…)[]]`) — tsc preserves the
/// fixed-prefix literal in that case. The carve-out is scoped to this
/// display path via the `preserve_booleans_in_rest_tuples` parameter so
/// inference/redeclaration paths still widen rest-tuple element booleans.
///
/// Use this only in diagnostic-formatting paths where the argument's literal
/// type carries no semantic meaning (the diagnostic is structural). Narrowing
/// flow displays should keep using `widen_type_for_display` so that
/// `string | false` doesn't collapse to `string | boolean`.
pub fn widen_argument_type_for_display(
    db: &dyn crate::construction::TypeDatabase,
    type_id: TypeId,
) -> TypeId {
    use rustc_hash::FxHashMap;
    let mut cache = FxHashMap::default();
    widen_type_cached(db, type_id, &mut cache, true, false, false, true, false)
}

/// Widen type for inference resolution: like `widen_type` but does NOT
/// recurse into function/callable parameter or return types.
///
/// tsc's `getInferredType` deep-widens fresh object literals but preserves
/// function types as-is. Widening function params in contravariant positions
/// (e.g., `(x: 1 | 2) => void` → `(x: number) => void`) creates a resolved T
/// that is structurally incompatible with the original arg type under strict
/// function type checking, causing false TS2322.
pub fn widen_type_for_inference(
    db: &dyn crate::construction::TypeDatabase,
    type_id: TypeId,
) -> TypeId {
    use rustc_hash::FxHashMap;
    let mut cache = FxHashMap::default();
    widen_type_cached(db, type_id, &mut cache, true, false, true, false, false)
}

/// Display-widen a type for TS2403 (subsequent variable declaration) messages.
///
/// Deep-widens fresh literal types nested inside compound shapes (function
/// return types, object property types) so the printer renders widened forms
/// like `{ x: number; y: number; }` rather than `{ x: 0; y: 0; }`. But
/// preserves top-level literal and union-of-literal types so explicit
/// annotations like `var x: 5; var x: 6;` keep their literal form (`'5'` /
/// `'6'`) instead of collapsing to `'number'` / `'number'` (which would also
/// self-suppress the diagnostic via the equal-display short-circuit in the
/// reporter).
pub fn display_widen_for_redeclaration(
    db: &dyn crate::construction::TypeDatabase,
    type_id: TypeId,
) -> TypeId {
    if matches!(
        db.lookup(type_id),
        Some(crate::types::TypeData::Literal(_) | crate::types::TypeData::Union(_))
    ) {
        return type_id;
    }
    widen_type_deep(db, type_id)
}

/// Widen a fresh `let`/`var` initializer type, recursing into union members.
///
/// Like [`widen_type`] but additionally widens fresh object/array members that
/// sit inside a top-level union, matching tsc's `getWidenedType`, which widens
/// every constituent of a union carrying the widening flag. The plain
/// [`widen_type`] entry only widens a union when it is a small union of bare
/// literals (`1 | 2`), so a union produced by a conditional over array literals
/// — `cond ? [1, 2, 3] : [4, 5]` → `(1 | 2 | 3)[] | (4 | 5)[]` — was returned
/// unchanged, leaving literal element types that collapse a later `.push`
/// parameter to `never` (the contravariant intersection of the per-arm element
/// types). With `widen_object_union_members`, each array constituent widens to
/// `number[]` and the union dedupes to `number[]`.
///
/// Object freshness is still respected: a non-fresh object constituent (from a
/// type alias or annotation) is left untouched, so `let y = aliasUnion` keeps
/// its declared literal members. This entry is reached only from the
/// fresh-initializer widening path (callers gate on
/// `is_fresh_literal_expression`), so widening the array constituents is safe —
/// they originate from fresh array literals.
pub fn widen_type_for_mutable_binding(
    db: &dyn crate::construction::TypeDatabase,
    type_id: TypeId,
) -> TypeId {
    use rustc_hash::FxHashMap;
    // Only top-level unions differ from `widen_type`; everything else (literals,
    // single arrays/tuples, objects) already widens identically, so defer to the
    // memoized general entry to keep the common path O(1).
    if !matches!(db.lookup(type_id), Some(crate::types::TypeData::Union(_))) {
        return widen_type(db, type_id);
    }
    let mut cache = FxHashMap::default();
    widen_type_cached(db, type_id, &mut cache, true, true, true, false, false)
}

/// Widen a `const` declaration's fresh initializer type the way tsc does.
///
/// `const` preserves a *top-level* primitive literal — `const c = "x"` is `"x"`,
/// and `const c = cond ? "x" : "y"` is `"x" | "y"` — but array, tuple, and
/// object-literal element positions are mutable, so their literal members still
/// widen: `const c = cond ? ["x"] : []` is `string[]`, not `("x")[]` (#14165,
/// remeda `errors.push(...)`). The plain [`widen_type_for_mutable_binding`]
/// (used for `let`/`var`) would over-widen the top-level literal union to
/// `string`, which is wrong for `const`.
///
/// Strategy: preserve a top-level literal / unique symbol; map a union member by
/// member (literal members preserved, compound members widened); and widen a
/// fresh array/tuple/object compound's members via the mutable-binding widener
/// (which respects object freshness). Reached only for fresh compound
/// initializers — the checker gates on `is_fresh_literal_expression`, so a
/// non-fresh initializer keeps its declared type.
pub fn widen_const_initializer(
    db: &dyn crate::construction::TypeDatabase,
    type_id: TypeId,
) -> TypeId {
    if type_id.is_intrinsic() {
        return type_id;
    }
    match db.lookup(type_id) {
        // Distribute over a union: literal members are preserved, array/object
        // members widen. `cond ? "x" : ["y"]` → `"x" | string[]`.
        Some(TypeData::Union(list_id)) => {
            let members = db.type_list(list_id);
            let mapped: Vec<TypeId> = members
                .iter()
                .map(|&m| widen_const_initializer(db, m))
                .collect();
            if mapped.iter().zip(members.iter()).all(|(a, b)| a == b) {
                type_id
            } else {
                db.union(mapped)
            }
        }
        // Mutable compounds: widen element / property literals (freshness-respecting).
        Some(
            TypeData::Array(_)
            | TypeData::Tuple(_)
            | TypeData::Object(_)
            | TypeData::ObjectWithIndex(_),
        ) => widen_type_for_mutable_binding(db, type_id),
        // A top-level primitive literal / unique symbol (`const` preserves it) and
        // everything else (functions, type parameters, applications, …) unchanged.
        _ => type_id,
    }
}

/// Deep-widen a type including inside function/callable signatures.
///
/// Unlike `widen_type` which skips Function/Callable types for performance
/// and correctness in the general case, this variant recurses into function
/// return types and parameter types. Used for TS2403 redeclaration checking
/// where `var fn = (s: string) => 3` should compare as `(s: string) => number`
/// against `var fn: (s: string) => number`.
pub fn widen_type_deep(db: &dyn crate::construction::TypeDatabase, type_id: TypeId) -> TypeId {
    // Fast path: intrinsics (except boolean literals)
    if type_id.is_intrinsic()
        && type_id != crate::types::TypeId::BOOLEAN_TRUE
        && type_id != crate::types::TypeId::BOOLEAN_FALSE
    {
        return type_id;
    }
    // Skip types that never contain widenable data, but NOT Function/Callable
    if matches!(
        db.lookup(type_id),
        Some(
            crate::types::TypeData::TypeParameter(_)
                | crate::types::TypeData::Enum(_, _)
                | crate::types::TypeData::Mapped(_)
                | crate::types::TypeData::Conditional(_)
                | crate::types::TypeData::Application(_)
                | crate::types::TypeData::Lazy(_)
                | crate::types::TypeData::IndexAccess(_, _)
                | crate::types::TypeData::KeyOf(_)
                | crate::types::TypeData::TemplateLiteral(_)
                | crate::types::TypeData::ThisType
                | crate::types::TypeData::Error
        )
    ) {
        return type_id;
    }
    use rustc_hash::FxHashMap;
    let mut cache = FxHashMap::default();
    widen_type_cached(db, type_id, &mut cache, true, true, false, false, false)
}

fn widen_type_cached(
    db: &dyn crate::construction::TypeDatabase,
    type_id: TypeId,
    // Keyed on `(TypeId, widen_boolean_intrinsics)` so the same compound
    // type processed under different boolean-widening contexts inside one
    // widening operation can't poison each other's results. The
    // `preserve_booleans_in_rest_tuples` carve-out flips
    // `widen_boolean_intrinsics` from `true` (outside a rest-tuple) to
    // `false` (inside) mid-traversal; without the flag in the key, the
    // first-processed version would short-circuit the second.
    cache: &mut rustc_hash::FxHashMap<(TypeId, bool), TypeId>,
    widen_boolean_intrinsics: bool,
    widen_functions: bool,
    widen_object_union_members: bool,
    // When true, boolean intrinsics inside the elements of a tuple that
    // carries a rest element are NOT widened — `[string, number, true,
    // ...(…)[]]` stays as-is. This is a display-only carve-out used by
    // `widen_argument_type_for_display` to match tsc's TS2345 source-type
    // rendering. All other widening paths (semantic widening, inference
    // resolution, redeclaration display) pass false so tuples like
    // `[true, ...string[]]` keep widening their fixed-prefix booleans, as
    // `inference/infer_resolve.rs:721` explicitly relies on.
    preserve_booleans_in_rest_tuples: bool,
    // When true, object types that are NOT fresh object literals are left
    // untouched (their literal property types are preserved, not widened).
    // This mirrors tsc's `getWidenedType`, which only widens types carrying
    // the widening flag (`FRESH_LITERAL` here): a declared/computed object such
    // as `{ kind: "other" }` keeps its literal members in diagnostics, while a
    // fresh object literal `{ kind: "other" }` written at an assignment still
    // widens to `{ kind: string }`. Display widening paths set this so they
    // match tsc; semantic/inference paths leave it false because they already
    // gate object-property widening on freshness via
    // `widen_object_union_members`.
    respect_object_freshness: bool,
) -> TypeId {
    // Fast path: most intrinsic types are never widened, but boolean
    // literal intrinsics (BOOLEAN_TRUE / BOOLEAN_FALSE) must widen to BOOLEAN.
    if (type_id == TypeId::BOOLEAN_TRUE || type_id == TypeId::BOOLEAN_FALSE)
        && widen_boolean_intrinsics
    {
        return TypeId::BOOLEAN;
    }
    if type_id.is_intrinsic() {
        return type_id;
    }

    if let Some(&cached) = cache.get(&(type_id, widen_boolean_intrinsics)) {
        return cached;
    }

    // Insert a sentinel before recursing to break cycles on recursive types
    // like `D<T> { recurse: D<T>; wrapped: D<D<T>>; }`. If we encounter
    // this type_id again during recursive widening, we return the original
    // type_id (no widening) instead of diverging.
    cache.insert((type_id, widen_boolean_intrinsics), type_id);

    let result = match db.lookup(type_id) {
        // String/Number/Boolean/BigInt literals widen to their primitives
        Some(TypeData::Literal(ref value)) => value.primitive_type_id(),

        // Unique Symbol widens to Symbol
        Some(TypeData::UniqueSymbol(_)) => TypeId::SYMBOL,

        // Unions: only widen if the union itself requires widening.
        // tsc's getWidenedType only widens types with the RequiresWidening flag.
        // A union like `1 | 2` from fresh expressions requires widening (→ number).
        // A union like `"fr" | "en" | "es"` from a type alias does NOT.
        //
        // We approximate tsc's RequiresWidening heuristic: a union requires widening
        // when it has ≤ 3 members, at least one of which is a literal type, and
        // all members are either literals or non-widenable intrinsics (undefined,
        // null, void). Larger unions are more likely from type aliases.
        // This handles: `1 | 2`, `true | false`, `"a" | 1`, `true | undefined`.
        Some(TypeData::Union(list_id)) => {
            let members = db.type_list(list_id);
            let is_fresh_member = |m: TypeId| -> bool {
                if m == TypeId::BOOLEAN_TRUE || m == TypeId::BOOLEAN_FALSE {
                    return true;
                }
                if m.is_intrinsic() {
                    return false;
                }
                matches!(
                    db.lookup(m),
                    Some(TypeData::Literal(_) | TypeData::UniqueSymbol(_))
                )
            };
            // Allow undefined/null/void as union members — they don't need
            // widening themselves but shouldn't prevent literal siblings from
            // being widened. E.g., `true | undefined` → `boolean | undefined`.
            let is_passthrough_intrinsic = |m: TypeId| -> bool {
                m == TypeId::UNDEFINED || m == TypeId::NULL || m == TypeId::VOID
            };
            let is_fresh_object_member = |m: TypeId| -> bool {
                if m.is_intrinsic() {
                    return false;
                }
                match db.lookup(m) {
                    Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => db
                        .object_shape(shape_id)
                        .flags
                        .contains(ObjectFlags::FRESH_LITERAL),
                    _ => false,
                }
            };
            let is_fresh_object_or_array_member = |m: TypeId| -> bool {
                if m.is_intrinsic() {
                    return false;
                }
                match db.lookup(m) {
                    Some(TypeData::Object(_) | TypeData::ObjectWithIndex(_)) => {
                        is_fresh_object_member(m)
                    }
                    Some(TypeData::Array(_) | TypeData::Tuple(_)) => true,
                    _ => false,
                }
            };
            let has_literal = members.iter().any(|&m| is_fresh_member(m));
            // A union of only fresh literals (`"a" | "b" | …`, optionally with
            // pass-through `undefined`/`null`/`void`). `small_fresh_union` caps
            // this at 3 members for the general/display/argument paths because a
            // larger literal union there is more likely to come from a type alias
            // (`type Lang = "fr" | "en" | "es" | "de"`) that must keep its
            // members. The inference and mutable-binding callers
            // (`widen_object_union_members == true`) already establish freshness
            // upstream — they only deep-widen array/tuple/object literals whose
            // candidate is fresh (not a `type`-annotated source) — so for them
            // the arity cap is wrong: tsc widens a fresh literal element union to
            // its primitive regardless of how many distinct literals it has
            // (`frz(["PATCH","POST","PUT","DELETE"])` → `Readonly<string[]>`).
            let all_fresh_literal_or_passthrough = has_literal
                && members
                    .iter()
                    .all(|&m| is_fresh_member(m) || is_passthrough_intrinsic(m));
            let small_fresh_union = all_fresh_literal_or_passthrough && members.len() <= 3;
            let fresh_literal_union =
                widen_object_union_members && all_fresh_literal_or_passthrough;
            let has_fresh_object_or_array_member =
                members.iter().any(|&m| is_fresh_object_or_array_member(m));
            if small_fresh_union
                || fresh_literal_union
                || (widen_object_union_members && has_fresh_object_or_array_member)
            {
                let mut members_to_widen = members.to_vec();
                if widen_object_union_members {
                    let fresh_object_members: Vec<TypeId> = members
                        .iter()
                        .copied()
                        .filter(|&member| is_fresh_object_member(member))
                        .collect();
                    if let Some(normalized_objects) =
                        super::expression_ops::normalize_fresh_object_literal_union_members(
                            db,
                            &fresh_object_members,
                        )
                    {
                        let mut normalized = normalized_objects.into_iter();
                        for member in &mut members_to_widen {
                            if is_fresh_object_member(*member)
                                && let Some(normalized_member) = normalized.next()
                            {
                                *member = normalized_member;
                            }
                        }
                    }
                }

                let widened_members: Vec<TypeId> = members_to_widen
                    .iter()
                    .map(|&m| {
                        widen_type_cached(
                            db,
                            m,
                            cache,
                            widen_boolean_intrinsics,
                            widen_functions,
                            widen_object_union_members,
                            preserve_booleans_in_rest_tuples,
                            respect_object_freshness,
                        )
                    })
                    .collect();
                // Preserve source order for diagnostic display: the canonical
                // union sort uses anonymous shape allocation order, which can
                // disagree with source order when normalization allocates new
                // shapes for some members but reuses existing ones for others.
                let origin_members = widened_members.clone();
                let widened = db.union(widened_members);
                display_provenance::record_union_origin(
                    db,
                    UnionOriginProvenance {
                        union_type_id: widened,
                        origin_members,
                    },
                );
                propagate_display_alias(db, type_id, widened);
                widened
            } else {
                type_id
            }
        }

        // Objects: recursively widen properties (critical for mutable variables)
        Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
            let shape = db.object_shape(shape_id);
            // In inference contexts (widen_object_union_members = true) and in
            // display contexts (respect_object_freshness = true), only widen fresh
            // object literals. Non-fresh objects (from type annotations or explicit
            // type constructions) are returned as-is, matching tsc's getWidenedType
            // which only widens types carrying the ContainsWideningType flag
            // (FRESH_LITERAL in tsz).
            if (widen_object_union_members || respect_object_freshness)
                && !shape.flags.contains(ObjectFlags::FRESH_LITERAL)
            {
                return type_id;
            }
            let mut new_props = Vec::with_capacity(shape.properties.len());
            let mut changed = false;
            let strip_fresh_display =
                widen_object_union_members && shape.flags.contains(ObjectFlags::FRESH_LITERAL);

            for prop in &shape.properties {
                // Rule: Readonly properties preserve their *own* primitive
                // literal type (so `class C { readonly x = 1 }` keeps
                // `readonly x = 1` in the .d.ts), but compound types
                // (objects, arrays, tuples) must still recurse to widen
                // their inner literals — tsc widens nested literals even
                // through readonly:
                //   class C { readonly n = { p: 1 } }    // → { p: number }
                //   class C { readonly a = [1, 2, 3] }   // → number[]
                // Likewise, the unique-symbol primitive carve-out remains
                // for readonly props: `readonly s: unique symbol` stays.
                // A `readonly` property keeps its *own* primitive literal type,
                // and a `non_widening` (regular) literal property — preserved at
                // construction from an `as const`/assertion source — must never
                // re-widen either, mirroring tsc's `getWidenedType` leaving
                // regular literals untouched. In both cases only the *top-level*
                // literal is preserved; compound values still recurse so nested
                // fresh literals widen as tsc does.
                let preserves_literal = prop.readonly || prop.non_widening;
                let widened_type = if preserves_literal
                    && readonly_property_preserves_top_level_type(db, prop.type_id)
                {
                    prop.type_id
                } else {
                    widen_type_cached(
                        db,
                        prop.type_id,
                        cache,
                        widen_boolean_intrinsics,
                        widen_functions,
                        widen_object_union_members,
                        preserve_booleans_in_rest_tuples,
                        respect_object_freshness,
                    )
                };

                // Write type follows read type logic.
                let widened_write_type = if preserves_literal
                    && readonly_property_preserves_top_level_type(db, prop.write_type)
                {
                    prop.write_type
                } else {
                    widen_type_cached(
                        db,
                        prop.write_type,
                        cache,
                        widen_boolean_intrinsics,
                        widen_functions,
                        widen_object_union_members,
                        preserve_booleans_in_rest_tuples,
                        respect_object_freshness,
                    )
                };

                if widened_type != prop.type_id || widened_write_type != prop.write_type {
                    changed = true;
                }
                let mut new_prop = prop.clone();
                new_prop.type_id = widened_type;
                new_prop.write_type = widened_write_type;
                new_props.push(new_prop);
            }

            if changed || strip_fresh_display {
                let mut flags = shape.flags;
                if strip_fresh_display {
                    flags.remove(ObjectFlags::FRESH_LITERAL);
                }

                let widened_type_id =
                    if shape.string_index.is_some() || shape.number_index.is_some() {
                        let mut new_shape = (*shape).clone();
                        new_shape.properties = new_props;
                        new_shape.flags = flags;
                        db.object_with_index(new_shape)
                    } else {
                        // Preserve symbol and flags so named types (interfaces,
                        // classes) retain their identity through widening.
                        db.object_with_flags_and_symbol(new_props, flags, shape.symbol)
                    };

                // Carry forward display properties from the original TypeId.
                if let Some(display_props) = db.get_display_properties(type_id) {
                    display_provenance::record_fresh_object_literal_display(
                        db,
                        FreshObjectLiteralDisplayProvenance {
                            type_id: widened_type_id,
                            properties: display_props.as_ref().clone(),
                        },
                    );
                }
                propagate_display_alias(db, type_id, widened_type_id);

                widened_type_id
            } else {
                type_id
            }
        }

        // Arrays: recursively widen element type
        Some(TypeData::Array(element_type)) => {
            let widened = widen_type_cached(
                db,
                element_type,
                cache,
                widen_boolean_intrinsics,
                widen_functions,
                widen_object_union_members,
                preserve_booleans_in_rest_tuples,
                respect_object_freshness,
            );
            if widened != element_type {
                let widened_arr = db.array(widened);
                propagate_display_alias(db, type_id, widened_arr);
                widened_arr
            } else {
                type_id
            }
        }

        // Tuples: recursively widen element types
        Some(TypeData::Tuple(tuple_list_id)) => {
            let elements = db.tuple_list(tuple_list_id);
            // tsc preserves boolean literal element types in tuples that have
            // a rest element (e.g. `[string, number, true, ...(…)[]]`) when
            // rendering call-argument displays. Honour that *only* in the
            // call-argument display path (`preserve_booleans_in_rest_tuples`)
            // so `widen_argument_type_for_display` widens `[1, 2, false, true]`
            // → `[number, number, boolean, boolean]` but keeps `[string,
            // number, true, ...(…)[]]` unchanged for
            // `argumentExpressionContextualTyping`. Other widening paths
            // (general semantic widening, inference resolution, redeclaration
            // display) skip this carve-out so e.g. `[true, ...string[]]`
            // continues to widen its fixed-prefix `true` to `boolean` — the
            // inference resolution path explicitly relies on this for
            // `infer_resolve.rs:721`.
            let widen_booleans_here = widen_boolean_intrinsics
                && !(preserve_booleans_in_rest_tuples && elements.iter().any(|elem| elem.rest));
            let mut new_elements = Vec::with_capacity(elements.len());
            let mut changed = false;
            for elem in elements.iter() {
                let widened = widen_type_cached(
                    db,
                    elem.type_id,
                    cache,
                    widen_booleans_here,
                    widen_functions,
                    widen_object_union_members,
                    preserve_booleans_in_rest_tuples,
                    respect_object_freshness,
                );
                if widened != elem.type_id {
                    changed = true;
                }
                let mut new_elem = *elem;
                new_elem.type_id = widened;
                new_elements.push(new_elem);
            }
            if changed {
                let widened_tuple = db.tuple(new_elements);
                propagate_display_alias(db, type_id, widened_tuple);
                widened_tuple
            } else {
                type_id
            }
        }

        // Functions: recursively widen parameter and return types for display contexts.
        Some(TypeData::Function(shape_id)) if widen_functions => {
            let shape = db.function_shape(shape_id);
            let mut widened_shape = shape.as_ref().clone();
            let mut changed = false;
            widened_shape.params = widened_shape
                .params
                .iter()
                .map(|param| {
                    let mut widened = *param;
                    widened.type_id = widen_type_cached(
                        db,
                        param.type_id,
                        cache,
                        widen_boolean_intrinsics,
                        widen_functions,
                        widen_object_union_members,
                        preserve_booleans_in_rest_tuples,
                        respect_object_freshness,
                    );
                    if widened.type_id != param.type_id {
                        changed = true;
                    }
                    widened
                })
                .collect();
            widened_shape.this_type = widened_shape.this_type.map(|this_ty| {
                let widened = widen_type_cached(
                    db,
                    this_ty,
                    cache,
                    widen_boolean_intrinsics,
                    widen_functions,
                    widen_object_union_members,
                    preserve_booleans_in_rest_tuples,
                    respect_object_freshness,
                );
                if widened != this_ty {
                    changed = true;
                }
                widened
            });
            let widened_return = widen_type_cached(
                db,
                widened_shape.return_type,
                cache,
                widen_boolean_intrinsics,
                widen_functions,
                widen_object_union_members,
                preserve_booleans_in_rest_tuples,
                respect_object_freshness,
            );
            if widened_return != widened_shape.return_type {
                changed = true;
            }
            widened_shape.return_type = widened_return;

            if changed {
                let widened_fn = db.function(widened_shape);
                propagate_display_alias(db, type_id, widened_fn);
                widened_fn
            } else {
                type_id
            }
        }

        // Callable objects: recursively widen all signature parameter/return types.
        Some(TypeData::Callable(shape_id)) if widen_functions => {
            let shape = db.callable_shape(shape_id);
            let mut widened_shape = shape.as_ref().clone();
            let mut changed = false;
            widened_shape.call_signatures = widened_shape
                .call_signatures
                .iter()
                .map(|sig| {
                    let mut widened_sig = sig.clone();
                    widened_sig.params = widened_sig
                        .params
                        .iter()
                        .map(|param| {
                            let mut widened = *param;
                            widened.type_id = widen_type_cached(
                                db,
                                param.type_id,
                                cache,
                                widen_boolean_intrinsics,
                                widen_functions,
                                widen_object_union_members,
                                preserve_booleans_in_rest_tuples,
                                respect_object_freshness,
                            );
                            if widened.type_id != param.type_id {
                                changed = true;
                            }
                            widened
                        })
                        .collect();
                    widened_sig.this_type = widened_sig.this_type.map(|this_ty| {
                        let widened = widen_type_cached(
                            db,
                            this_ty,
                            cache,
                            widen_boolean_intrinsics,
                            widen_functions,
                            widen_object_union_members,
                            preserve_booleans_in_rest_tuples,
                            respect_object_freshness,
                        );
                        if widened != this_ty {
                            changed = true;
                        }
                        widened
                    });
                    let widened_return = widen_type_cached(
                        db,
                        widened_sig.return_type,
                        cache,
                        widen_boolean_intrinsics,
                        widen_functions,
                        widen_object_union_members,
                        preserve_booleans_in_rest_tuples,
                        respect_object_freshness,
                    );
                    if widened_return != widened_sig.return_type {
                        changed = true;
                    }
                    widened_sig.return_type = widened_return;
                    widened_sig
                })
                .collect();

            if changed {
                let widened_callable = db.callable(widened_shape);
                propagate_display_alias(db, type_id, widened_callable);
                widened_callable
            } else {
                type_id
            }
        }

        // All other types (including Function/Callable when widen_functions is false)
        // are returned as-is. When widen_functions is false, tsc's getInferredType
        // does NOT recurse into function parameter types during deep-widening.
        // Widening function params changes contravariant positions
        // (e.g., `(x: 1 | 2) => void` → `(x: number) => void`), causing false TS2322.
        _ => type_id,
    };

    cache.insert((type_id, widen_boolean_intrinsics), result);
    result
}

/// Widen only object literal property types (not top-level types or union members).
///
/// This is used during inference resolution to match TypeScript's behavior:
/// when an object literal like `{ c: false }` is inferred against a bare type
/// parameter `T`, the property literal types are widened (`{ c: boolean }`).
/// However, top-level union types like `"foo" | "bar"` must NOT be widened
/// (they should stay as literal unions for type parameter inference).
///
/// This differs from `widen_type` which recursively widens everything including
/// union members and direct literals. This function only enters objects/arrays/tuples.
pub(crate) fn widen_object_literal_properties(
    db: &dyn crate::construction::TypeDatabase,
    type_id: TypeId,
) -> TypeId {
    if type_id.is_intrinsic() {
        return type_id;
    }
    match db.lookup(type_id) {
        // Objects: recursively widen mutable property types
        Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
            let shape = db.object_shape(shape_id);
            // Only fresh object literals carry tsc's widening flag
            // (`ObjectFlags.ContainsWideningType`, `FRESH_LITERAL` here). A
            // non-fresh object — a declared/annotated type, an alias instance,
            // or an object-spread result (`{ ...node }`, which tsz interns
            // non-fresh) — keeps its literal property types under
            // `getWidenedType`. Widening such a source while inferring a bare
            // type parameter would turn `{ kind: "X" }` into `{ kind: string }`
            // and break identity downstream (false TS2322/TS2345 on builder
            // factories like Kysely's `cloneWith`/`create`).
            if !shape.flags.contains(ObjectFlags::FRESH_LITERAL) {
                return type_id;
            }
            let mut new_props = Vec::with_capacity(shape.properties.len());
            let mut changed = false;

            for prop in &shape.properties {
                // `non_widening` properties hold a regular (non-widening) literal
                // preserved from an `as const`/assertion source; tsc's
                // `getWidenedType` never widens those, so skip them alongside
                // readonly properties.
                let preserve = prop.readonly || prop.non_widening;
                let widened_type = if preserve {
                    prop.type_id
                } else {
                    widen_type(db, prop.type_id)
                };
                let widened_write_type = if preserve {
                    prop.write_type
                } else {
                    widen_type(db, prop.write_type)
                };
                if widened_type != prop.type_id || widened_write_type != prop.write_type {
                    changed = true;
                }
                let mut new_prop = prop.clone();
                new_prop.type_id = widened_type;
                new_prop.write_type = widened_write_type;
                new_props.push(new_prop);
            }

            if changed {
                if shape.string_index.is_some() || shape.number_index.is_some() {
                    let mut new_shape = (*shape).clone();
                    new_shape.properties = new_props;
                    db.object_with_index(new_shape)
                } else {
                    // Preserve symbol and flags so named types retain identity.
                    db.object_with_flags_and_symbol(new_props, shape.flags, shape.symbol)
                }
            } else {
                type_id
            }
        }

        // All other types pass through unchanged — particularly unions of
        // string/number literals must NOT be widened here.
        _ => type_id,
    }
}

/// Get the base type of a literal type for comparison operators.
///
/// Matches TypeScript's `getBaseTypeOfLiteralTypeForComparison`:
/// - String literals, template literals, string intrinsics → `string`
/// - Number literals → `number`
/// - `BigInt` literals → `bigint`
/// - Boolean literals → `boolean`
/// - Enum types → recursively widen their member union
/// - Union types → recursively map each member
/// - Everything else → unchanged
///
/// Used by relational operators (`<`, `>`, `<=`, `>=`) to normalize types
/// before comparability checks. This is distinct from general widening because
/// it also handles enum types and template literals.
pub fn get_base_type_for_comparison(
    db: &dyn crate::construction::TypeDatabase,
    type_id: TypeId,
) -> TypeId {
    // BOOLEAN_TRUE/FALSE are intrinsic IDs that look up as `Literal(Boolean)`,
    // which the match below widens to `BOOLEAN`. All other intrinsics fall
    // through to `_ => type_id`, so they can short-circuit the lookup.
    if type_id.is_intrinsic() && type_id != TypeId::BOOLEAN_TRUE && type_id != TypeId::BOOLEAN_FALSE
    {
        return type_id;
    }
    match db.lookup(type_id) {
        // String/Number/Boolean/BigInt literals widen to their primitives
        Some(TypeData::Literal(ref value)) => value.primitive_type_id(),

        // Enum types: recursively widen their member union
        // (numeric enums → number, string enums → string)
        Some(TypeData::Enum(_, member_type_id)) => get_base_type_for_comparison(db, member_type_id),

        // Template literals and string intrinsics (Uppercase<T>, etc.) → string
        Some(TypeData::TemplateLiteral(_) | TypeData::StringIntrinsic { .. }) => TypeId::STRING,

        // Type parameters: resolve through constraint to determine comparison family.
        // This ensures that e.g. `T extends "a" | "b"` has comparison base `string`,
        // matching the base of literal `"x"`, so the TS2367 display preserves
        // literal detail for same-family comparisons (tsc shows `T` and `"x"`,
        // not `T` and `string`).
        Some(TypeData::TypeParameter(ref info)) => {
            if let Some(constraint) = info.constraint {
                get_base_type_for_comparison(db, constraint)
            } else {
                type_id
            }
        }

        // Unions: recursively map all members
        Some(TypeData::Union(list_id)) => {
            let members = db.type_list(list_id);
            let mapped: Vec<TypeId> = members
                .iter()
                .map(|&m| get_base_type_for_comparison(db, m))
                .collect();
            db.union(mapped)
        }

        // Everything else unchanged
        _ => type_id,
    }
}

/// Widen only literal types to their base primitive types.
///
/// This is more targeted than `get_base_type_for_comparison`:
/// - String/Number/Boolean/BigInt literals → their primitive types
/// - Unions → recursively map members
/// - Everything else (including enums, template literals) → unchanged
///
/// Used for binary operator error messages where tsc shows widened types
/// for literal operands but preserves enum type names.
pub fn widen_literal_type(db: &dyn crate::construction::TypeDatabase, type_id: TypeId) -> TypeId {
    // `FxHashSet::default()` does not allocate until the first insert, and only
    // the union arm inserts, so the common non-union inputs stay allocation-free.
    widen_literal_type_tracked(db, type_id, &mut FxHashSet::default())
}

/// Rebuild an object type from `original` (whose shape is `shape`) substituting
/// `new_props` for its properties, preserving index signatures, flags
/// (including `FRESH_LITERAL`), declaring symbol, and display provenance.
///
/// This is the same reconstruction the object branch of `widen_type_cached`
/// performs; it is exposed so AST-driven callers (e.g. return-type inference
/// that preserves const-asserted property literals) can widen a subset of an
/// object literal's properties without losing the object's freshness/identity
/// metadata. Returns `original` unchanged when the properties are unchanged.
pub fn rebuild_object_with_shape_metadata(
    db: &dyn crate::construction::TypeDatabase,
    original: TypeId,
    shape: &crate::types::ObjectShape,
    new_props: Vec<crate::types::PropertyInfo>,
) -> TypeId {
    if new_props == shape.properties {
        return original;
    }

    let rebuilt = if shape.string_index.is_some() || shape.number_index.is_some() {
        let mut new_shape = shape.clone();
        new_shape.properties = new_props;
        db.object_with_index(new_shape)
    } else {
        db.object_with_flags_and_symbol(new_props, shape.flags, shape.symbol)
    };

    if rebuilt != original {
        if let Some(display_props) = db.get_display_properties(original) {
            display_provenance::record_fresh_object_literal_display(
                db,
                FreshObjectLiteralDisplayProvenance {
                    type_id: rebuilt,
                    properties: display_props.as_ref().clone(),
                },
            );
        }
        propagate_display_alias(db, original, rebuilt);
    }

    rebuilt
}

/// `widen_literal_type` with an on-stack ancestor set guarding union recursion.
///
/// A union's display-origin provenance can transitively reference the union
/// itself (a cyclic origin chain). Recursing into such members through the
/// `Union` arm would otherwise overflow the stack. `on_stack` records the
/// unions currently being widened on this call path; revisiting one returns it
/// unchanged (it is already being widened by an ancestor frame). Entries are
/// inserted on enter and removed on exit so non-cyclic diamonds — the same
/// union reached by two independent member paths — are still widened on each
/// path.
fn widen_literal_type_tracked(
    db: &dyn crate::construction::TypeDatabase,
    type_id: TypeId,
    on_stack: &mut FxHashSet<TypeId>,
) -> TypeId {
    if type_id == TypeId::BOOLEAN_TRUE || type_id == TypeId::BOOLEAN_FALSE {
        return TypeId::BOOLEAN;
    }
    // Other intrinsics resolve to TypeData::Intrinsic; the match below would
    // fall through to `_ => type_id`. `is_intrinsic()` is a free TypeId-range
    // check — skip the dyn lookup.
    if type_id.is_intrinsic() {
        return type_id;
    }

    match db.lookup(type_id) {
        Some(TypeData::Literal(ref value)) => value.primitive_type_id(),

        Some(TypeData::Union(list_id)) => {
            if !on_stack.insert(type_id) {
                // Cyclic union origin: this union is an ancestor on the current
                // widening path. Returning it unchanged breaks the cycle.
                return type_id;
            }
            let result = widen_union_literal_members(db, type_id, list_id, on_stack);
            on_stack.remove(&type_id);
            result
        }

        _ => type_id,
    }
}

/// Widen the members of a literal-bearing union, threading the on-stack
/// ancestor set so nested unions stay cycle-guarded. Returns the union
/// unchanged when no member widened.
fn widen_union_literal_members(
    db: &dyn crate::construction::TypeDatabase,
    type_id: TypeId,
    list_id: crate::types::TypeListId,
    on_stack: &mut FxHashSet<TypeId>,
) -> TypeId {
    let canonical_members = db.type_list(list_id);
    let origin_members = db.get_union_origin(type_id);
    let members = origin_members
        .as_deref()
        .map_or(canonical_members.as_ref(), Vec::as_slice);
    let mut mapped = None;
    for (index, &member) in members.iter().enumerate() {
        let widened = widen_literal_type_tracked(db, member, on_stack);
        if widened != member && mapped.is_none() {
            let mut out = Vec::with_capacity(members.len());
            out.extend_from_slice(&members[..index]);
            mapped = Some(out);
        }
        if let Some(mapped) = mapped.as_mut() {
            mapped.push(widened);
        }
    }

    let Some(mapped) = mapped else {
        return type_id;
    };

    let result = db.union_from_slice(&mapped);
    display_provenance::record_union_origin(
        db,
        UnionOriginProvenance {
            union_type_id: result,
            origin_members: mapped,
        },
    );
    result
}

/// Combine the source constituents of a union that did not match any structured
/// arm of a union target into a single inference candidate for the lone naked
/// type variable, mirroring tsc's
/// `inferFromTypes(getUnionType(unmatched), nakedTypeVariable)`.
///
/// A single constituent is returned unchanged so the resolver still sees a fresh
/// literal and widens it uniformly with the structured-arm candidates. Several
/// constituents are unioned, but because that union is no longer a fresh literal
/// its members are widened here (unless `in_readonly_source` suppresses freshness,
/// as in an `as const` argument) to mirror tsc's `getWidenedLiteralType`.
pub fn union_unmatched_naked_candidate(
    db: &dyn crate::construction::TypeDatabase,
    members: Vec<TypeId>,
    in_readonly_source: bool,
) -> TypeId {
    if members.len() > 1 && !in_readonly_source {
        let widened: Vec<TypeId> = members
            .iter()
            .map(|&member| widen_literal_type(db, member))
            .collect();
        crate::utils::union_or_single(db, widened)
    } else {
        crate::utils::union_or_single(db, members)
    }
}

/// Widen number and boolean literal types but preserve string and bigint literals.
///
/// tsc's TS2367 diagnostic uses widened types for number/boolean operands
/// (e.g., `true` → `boolean`, `0` → `number`) but preserves string/bigint
/// literal types in the message text.
#[allow(dead_code)] // Reserved for TS2367 diagnostic message formatting
pub(crate) fn widen_non_string_bigint_literal(
    db: &dyn crate::construction::TypeDatabase,
    type_id: TypeId,
) -> TypeId {
    if type_id == TypeId::BOOLEAN_TRUE || type_id == TypeId::BOOLEAN_FALSE {
        return TypeId::BOOLEAN;
    }
    if type_id.is_intrinsic() {
        return type_id;
    }
    match db.lookup(type_id) {
        Some(TypeData::Literal(ref value)) => match value {
            crate::LiteralValue::Number(_) => TypeId::NUMBER,
            crate::LiteralValue::Boolean(_) => TypeId::BOOLEAN,
            crate::LiteralValue::String(_) | crate::LiteralValue::BigInt(_) => type_id,
        },
        _ => type_id,
    }
}

/// Apply `as const` assertion to a type.
///
/// This function transforms a type to its const-asserted form:
/// - Literals: Preserved as-is
/// - Arrays: Converted to readonly tuples
/// - Tuples: Marked readonly, elements recursively const-asserted
/// - Objects: All properties marked readonly, recursively const-asserted
/// - Other types: Preserved as-is (any, unknown, primitives, etc.)
///
/// # Example
///
/// ```text
/// use crate::operations::widening::apply_const_assertion;
///
/// // [1, 2] as const becomes readonly [1, 2] (tuple)
/// let array_type = interner.array(interner.literal_number(1));
/// let const_array = apply_const_assertion(&interner, array_type);
/// ```
pub fn apply_const_assertion(
    db: &dyn crate::construction::TypeDatabase,
    type_id: TypeId,
) -> TypeId {
    use crate::visitor::ConstAssertionVisitor;
    let mut visitor = ConstAssertionVisitor::new(db);
    visitor.apply_const_assertion(type_id)
}

#[cfg(test)]
#[path = "../../tests/widening_tests.rs"]
mod tests;
