use crate::construction::TypeDatabase;
use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};
use crate::type_queries::{get_array_element_type, get_callable_shape_for_type, get_union_members};
use crate::types::{CallSignature, ParamInfo, TypeData, TypeId};
use tsz_common::Atom;

const MAX_COMPARABILITY_DEPTH: u32 = 5;

/// Named recursion-depth state for comparability walks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ComparabilityDepthState {
    /// The current recursion depth is still inside the existing walk budget.
    WithinLimit,
    /// The current recursion depth exceeded the existing walk budget.
    LimitExceeded,
}

impl ComparabilityDepthState {
    const fn from_depth(depth: u32) -> Self {
        if depth > MAX_COMPARABILITY_DEPTH {
            Self::LimitExceeded
        } else {
            Self::WithinLimit
        }
    }

    const fn fallback_result(self) -> Option<bool> {
        match self {
            Self::WithinLimit => None,
            Self::LimitExceeded => Some(false),
        }
    }
}

/// Check if two types are "comparable" for TS2352 type assertion overlap check.
///
/// TSC uses `isTypeComparableTo` which is more relaxed than assignability.
/// Types are comparable if:
/// 1. They share at least one common object property name
/// 2. One is a base primitive type and the other is a literal/union of that primitive
/// 3. For union types, any member is comparable to the other type
///
/// This prevents false TS2352 errors on valid type assertions.
pub fn types_are_comparable(db: &dyn TypeDatabase, source: TypeId, target: TypeId) -> bool {
    types_are_comparable_inner(db, source, target, 0)
}

/// Check if two types are comparable for type assertion purposes (TS2352).
///
/// This is more permissive than the standard `types_are_comparable` - it only
/// requires that the types share at least one common property with comparable
/// types. It does NOT require all target properties to exist in the source.
///
/// This prevents false TS2352 errors on valid type assertions like:
/// - `{ required1: "hello" } as Foo` where Foo has additional required properties
/// - `{ payload: 'any-string' } as Action<'ACTION_A', string>`
pub fn types_are_comparable_for_assertion(
    db: &dyn TypeDatabase,
    source: TypeId,
    target: TypeId,
) -> bool {
    types_are_comparable_for_assertion_inner(db, source, target, 0, false)
}

/// `nested` tracks whether we have descended into an object property or a
/// tuple/array element. tsc's `checkAssertionWorker` widens only the
/// *top-level* assertion source via `getWidenedType`, so two distinct literals
/// that appear as the direct operands of an assertion (e.g. `"x" as "y"`, or a
/// member of the top-level union in `"x" as "y" | "z"`) are treated as
/// overlapping, whereas distinct literals nested inside a shared property
/// (e.g. `{ k: "a" } as { k: "b" }`) are *not* widened and therefore do not
/// overlap. Union/intersection/readonly/enum decomposition of the top-level
/// types keeps `nested` unchanged; only descending through a property or
/// element sets it.
pub(in crate::type_queries) fn types_are_comparable_for_assertion_inner(
    db: &dyn TypeDatabase,
    source: TypeId,
    target: TypeId,
    depth: u32,
    nested: bool,
) -> bool {
    if let Some(result) = ComparabilityDepthState::from_depth(depth).fallback_result() {
        return result;
    }

    // Same type is always comparable
    if source == target {
        return true;
    }

    // Cache the structural views once; this function pattern-matches both
    // operands repeatedly across the decomposition cases below.
    let source_data = db.lookup(source);
    let target_data = db.lookup(target);

    // Nested distinct literals do not overlap. The equal case is handled above,
    // so reaching here with two literal operands while `nested` means the
    // values differ; tsc does not widen nested literals, so `"a"` and `"b"` (or
    // `1` and `2`) are not comparable as shared property/element types. Enum
    // literals are `TypeData::Enum`, not `TypeData::Literal`, so same-enum
    // member comparability is unaffected.
    if nested
        && matches!(source_data, Some(TypeData::Literal(_)))
        && matches!(target_data, Some(TypeData::Literal(_)))
    {
        return false;
    }

    // `never` is comparable to any type (it's the bottom type, subtype of all types).
    // `any` and `unknown` are also comparable to everything.
    if source == TypeId::NEVER
        || target == TypeId::NEVER
        || source == TypeId::ANY
        || target == TypeId::ANY
        || source == TypeId::UNKNOWN
        || target == TypeId::UNKNOWN
    {
        return true;
    }

    // Handle Lazy types (unresolved semantic references like interface names).
    // Only assume comparable when BOTH are Lazy at depth > 0 — we can't
    // structurally compare two opaque references so we conservatively assume
    // overlap (avoids false TS2352). When only ONE side is Lazy, fall through
    // to the structural property check. The Lazy type will have no extractable
    // properties, so it correctly returns "not comparable" for cases like
    // comparing `{z: any}` (concrete Object) with `T1` (empty interface ref).
    //
    // NOTE: callers should pre-resolve Lazy types (via checker evaluation) before
    // invoking this function to avoid false TS2352 on assertions like
    // `{mode: ""} as UserSettings` where a nested interface property stays Lazy.
    let source_is_lazy = matches!(source_data, Some(TypeData::Lazy(_)));
    let target_is_lazy = matches!(target_data, Some(TypeData::Lazy(_)));
    if depth > 0 && source_is_lazy && target_is_lazy {
        return true;
    }

    // Unwrap ReadonlyType wrappers
    if let Some(TypeData::ReadonlyType(inner)) = source_data {
        return types_are_comparable_for_assertion_inner(db, inner, target, depth + 1, nested);
    }
    if let Some(TypeData::ReadonlyType(inner)) = target_data {
        return types_are_comparable_for_assertion_inner(db, source, inner, depth + 1, nested);
    }

    // Check union types
    if let Some(TypeData::Union(list_id)) = source_data {
        let members = db.type_list(list_id);
        return members
            .iter()
            .any(|&m| types_are_comparable_for_assertion_inner(db, m, target, depth + 1, nested));
    }
    if let Some(TypeData::Union(list_id)) = target_data {
        let members = db.type_list(list_id);
        return members
            .iter()
            .any(|&m| types_are_comparable_for_assertion_inner(db, source, m, depth + 1, nested));
    }

    // For intersection source S1 & S2 & ... & Sn: any member comparable to target suffices.
    // For intersection target T1 & T2 & ... & Tn: source must be comparable to every member
    // (tsc's eachTypeRelatedToType via comparableRelation).
    if let Some(TypeData::Intersection(list_id)) = source_data {
        let members = db.type_list(list_id);
        return members
            .iter()
            .any(|&m| types_are_comparable_for_assertion_inner(db, m, target, depth + 1, nested));
    }
    if let Some(TypeData::Intersection(list_id)) = target_data {
        let members = db.type_list(list_id);
        return members
            .iter()
            .all(|&m| types_are_comparable_for_assertion_inner(db, source, m, depth + 1, nested));
    }

    // Enum comparability: unwrap to member type union, matching
    // `types_are_comparable_inner` behavior.
    if let Some(TypeData::Enum(_def_id, members_type_id)) = source_data {
        return types_are_comparable_for_assertion_inner(
            db,
            members_type_id,
            target,
            depth + 1,
            nested,
        );
    }
    if let Some(TypeData::Enum(_def_id, members_type_id)) = target_data {
        return types_are_comparable_for_assertion_inner(
            db,
            source,
            members_type_id,
            depth + 1,
            nested,
        );
    }

    // Check primitive ↔ literal comparability
    if is_primitive_comparable(db, source, target) || is_primitive_comparable(db, target, source) {
        return true;
    }

    // `void` and `undefined` overlap in tsc's comparable relation: `tsc`'s
    // `isSimpleTypeRelatedTo` relates an `undefined` source to a `void` target,
    // and `checkAssertionWorker` runs comparability in both directions, so an
    // assertion between `void` and `undefined` (either way) is accepted. tsz's
    // assertion descent only checks one direction at a given nesting level, so
    // the rule is made mutual here. It is consulted at every recursion level —
    // both as top-level operands and in the contravariant callback-parameter
    // descent reached via `signature_param_types_are_comparable_for_assertion`
    // — which is the position the zustand `Thenable -> Promise` cast trips:
    // `then(cb: (v: undefined) => unknown)` vs `then(onfulfilled?: (value: void)
    // => unknown)` compares `undefined` against `void`.
    if is_void_undefined_overlap(source, target) {
        return true;
    }

    if type_param_primitive_comparable_with_constraint(db, target, source)
        || type_param_primitive_comparable_with_constraint(db, source, target)
    {
        return true;
    }

    // A template-literal type (and string-intrinsic mapping type such as
    // `Uppercase<S>`) is a subtype of `string`. For assertion comparability
    // tsc widens the source literal to its base primitive, so any string-domain
    // type (`string`, a string literal, a template-literal, or a string
    // intrinsic) sufficiently overlaps a template-literal/string-intrinsic
    // target — e.g. `"x" as `a${number}b`` is legal even though the literal
    // text does not match the pattern. This is the assertion-only widening
    // rule; the strict comparable relation is intentionally unchanged.
    if is_string_domain_type(db, source)
        && is_string_domain_type(db, target)
        && (is_template_or_string_intrinsic(db, source)
            || is_template_or_string_intrinsic(db, target))
    {
        return true;
    }

    // The `object` primitive overlaps with any non-primitive type, including
    // `{}` and arbitrary object/array shapes. tsc's `isTypeComparableTo`
    // treats `object` as a supertype of all object-like values for assertion
    // purposes. Without this special case, `{} as T` with `T extends object`
    // falls through to the property-overlap check, which returns false
    // because both sides have empty extractable property lists.
    if (source == TypeId::OBJECT && is_object_like_for_assertion(db, target))
        || (target == TypeId::OBJECT && is_object_like_for_assertion(db, source))
    {
        return true;
    }

    // `keyof T` (the `KeyOf` type operator) reduces to a subset of
    // `string | number | symbol`, so for assertion overlap purposes it is
    // comparable to any of those primitives or their literals. tsc's
    // `isTypeComparableTo` walks `keyof T` to its key-space union; without
    // this case, an assertion like `(k as string)` where `k: keyof T` falls
    // through to the property-overlap check (KeyOf has no extractable
    // properties) and emits a false-positive TS2352.
    if is_keyof_to_string_number_symbol(db, source, target)
        || is_keyof_to_string_number_symbol(db, target, source)
    {
        return true;
    }

    // A requirement-free object — the empty `{}`, an all-optional object, or a
    // universal index-signature record like `Record<PropertyKey, unknown>` — is
    // comparable to any type parameter whose constraint contains an object-like
    // or `object`-primitive member. tsc's `isTypeComparableTo` walks the type
    // parameter's constraint when the source is a "wide" object type. We narrow
    // this to sources with NO required named member — fully unwrapping for any
    // source would over-permit `B as T extends A` (genericTypeAssertions4.ts),
    // where `B`'s required `bar` must still report TS2352.
    if is_object_without_required_members(db, source)
        && let Some(TypeData::TypeParameter(info)) = db.lookup(target)
        && let Some(constraint) = info.constraint
    {
        return types_are_comparable_for_assertion_inner(db, source, constraint, depth + 1, nested);
    }
    if is_object_without_required_members(db, target)
        && let Some(TypeData::TypeParameter(info)) = db.lookup(source)
        && let Some(constraint) = info.constraint
    {
        return types_are_comparable_for_assertion_inner(db, constraint, target, depth + 1, nested);
    }

    if callable_signatures_overlap_for_assertion(db, source, target, depth) {
        return true;
    }

    // A callable asserted to a bare type parameter whose constraint is (or
    // includes) function types overlaps iff the callable is assertion-comparable
    // to that constraint. tsc resolves the bare type-parameter target to its
    // base constraint and runs the per-member overlap decomposition; resolving
    // it here lets the union handling above and the callable-signature overlap
    // below apply (e.g. `(fn as F)` where
    // `F extends ((...a: any[]) => any) | ((...a: any[]) => void)`). Gating on a
    // *direct callable* on the other side keeps this from over-permitting
    // object/primitive sources against a type-parameter constraint (which the
    // narrower object-without-required-members rule above already governs), and
    // preserves the negative control: a `string` source stays incomparable to a
    // function-union constraint. The symmetric source case covers a generic
    // source asserted to a concrete callable.
    if is_direct_callable_for_assertion(db, source)
        && let Some(TypeData::TypeParameter(info)) = target_data
        && let Some(constraint) = informative_type_param_constraint(info.constraint)
        && types_are_comparable_for_assertion_inner(db, source, constraint, depth + 1, nested)
    {
        return true;
    }
    if is_direct_callable_for_assertion(db, target)
        && let Some(TypeData::TypeParameter(info)) = source_data
        && let Some(constraint) = informative_type_param_constraint(info.constraint)
        && types_are_comparable_for_assertion_inner(db, constraint, target, depth + 1, nested)
    {
        return true;
    }

    // A deferred conditional (or an indexed access whose object base is a
    // deferred conditional) has no extractable properties, so it would fall
    // through to the property-overlap check and report a false TS2352. tsc
    // compares against `getBaseConstraintOfType` — the union of both branch
    // results — so resolve that base constraint and retry the overlap check.
    // This covers `Box<T>[keyof Box<T>] as string` (tanstack-router
    // `Matches.ts`) and the casting-into sibling.
    if let Some(source_constraint) = deferred_conditional_assertion_constraint(db, source)
        && source_constraint != source
        && types_are_comparable_for_assertion_inner(
            db,
            source_constraint,
            target,
            depth + 1,
            nested,
        )
    {
        return true;
    }
    if let Some(target_constraint) = deferred_conditional_assertion_constraint(db, target)
        && target_constraint != target
        && types_are_comparable_for_assertion_inner(
            db,
            source,
            target_constraint,
            depth + 1,
            nested,
        )
    {
        return true;
    }

    // For type assertions, only check that overlapping properties are comparable.
    // Do NOT require all target properties to exist in the source.
    super::super::assertion_overlap::types_have_common_properties_relaxed(db, source, target, depth)
}

/// Base constraint of a deferred conditional (or an indexed access whose object
/// is a deferred conditional), for assertion comparability.
///
/// For a `Conditional`, this is the union of its branch results (tsc's
/// `getBaseConstraintOfType`). For `Obj[Key]` where `Obj` is a deferred
/// conditional, it is `branchUnion[Key]` evaluated — e.g.
/// `Box<T>[keyof Box<T>]` with `Box<T> = T extends string ? { a: T } : { a: string }`
/// resolves to `({ a: T } | { a: string })[keyof …]` = `T | string`, a
/// string-domain type comparable to `string`. Returns `None` when no deferred
/// conditional base is involved (so non-conditional types skip the work).
fn deferred_conditional_assertion_constraint(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<TypeId> {
    use crate::evaluation::evaluate::{evaluate_index_access, evaluate_type};
    use crate::type_queries::conditional_branch_union_constraint;

    // Resolve the conditional base constraint of `ty`, evaluating it first so an
    // alias `Application` (e.g. `Box<T>`) collapses to its `Conditional` body.
    let conditional_constraint = |db: &dyn TypeDatabase, ty: TypeId| -> Option<TypeId> {
        conditional_branch_union_constraint(db, ty)
            .or_else(|| conditional_branch_union_constraint(db, evaluate_type(db, ty)))
    };

    if let Some(constraint) = conditional_constraint(db, type_id) {
        return Some(evaluate_type(db, constraint));
    }
    // `Obj[Key]` where `Obj` is a deferred conditional: substitute the object's
    // branch-union base constraint and re-evaluate the indexed access. Evaluate
    // `type_id` first so an alias application surfaces the underlying indexed
    // access (`Member<T>` -> `Box<T>[keyof Box<T>]`).
    let evaluated = evaluate_type(db, type_id);
    let ia = match db.lookup(evaluated) {
        Some(TypeData::IndexAccess(o, k)) => Some((o, k)),
        _ => match db.lookup(type_id) {
            Some(TypeData::IndexAccess(o, k)) => Some((o, k)),
            _ => None,
        },
    };
    if let Some((object_type, key_type)) = ia {
        let object_constraint = conditional_constraint(db, object_type)?;
        let resolved = evaluate_index_access(db, object_constraint, key_type);
        if resolved != evaluated && resolved != type_id && resolved != TypeId::ERROR {
            return Some(resolved);
        }
    }
    None
}

/// `void` and `undefined` are mutually comparable for the TS2352 assertion
/// relation. tsc's `isSimpleTypeRelatedTo` relates an `undefined` source to a
/// `void` target, and the assertion check (`checkAssertionWorker`) runs
/// comparability in both directions, so an assertion `void as undefined` or
/// `undefined as void` — at the top level or nested in a shared property /
/// element / callback parameter — is accepted. This is a value-domain overlap
/// (both inhabit only the `undefined` value), distinct from the
/// primitive-vs-literal widening handled by `is_primitive_comparable`, so it is
/// checked directly and independent of variance position.
const fn is_void_undefined_overlap(a: TypeId, b: TypeId) -> bool {
    matches!(
        (a, b),
        (TypeId::VOID, TypeId::UNDEFINED) | (TypeId::UNDEFINED, TypeId::VOID)
    )
}

fn type_param_primitive_comparable_with_constraint(
    db: &dyn TypeDatabase,
    type_param: TypeId,
    other: TypeId,
) -> bool {
    let Some(TypeData::TypeParameter(info)) = db.lookup(type_param) else {
        return false;
    };
    let Some(constraint) = informative_type_param_constraint(info.constraint) else {
        return false;
    };
    is_primitive_comparable(db, other, constraint) || is_primitive_comparable(db, constraint, other)
}

/// A type parameter's assertion-relevant base constraint: its declared
/// constraint, unless that constraint is the uninformative `any`/`unknown`
/// (which carries no structural overlap signal for the TS2352 check).
fn informative_type_param_constraint(constraint: Option<TypeId>) -> Option<TypeId> {
    constraint.filter(|&c| c != TypeId::ANY && c != TypeId::UNKNOWN)
}

fn callable_signatures_overlap_for_assertion(
    db: &dyn TypeDatabase,
    source: TypeId,
    target: TypeId,
    depth: u32,
) -> bool {
    if !is_direct_callable_for_assertion(db, source)
        || !is_direct_callable_for_assertion(db, target)
    {
        return false;
    }

    let Some(source_shape) = get_callable_shape_for_type(db, source) else {
        return false;
    };
    let Some(target_shape) = get_callable_shape_for_type(db, target) else {
        return false;
    };

    source_shape.call_signatures.iter().any(|source_sig| {
        target_shape.call_signatures.iter().any(|target_sig| {
            assertion_signatures_are_comparable(db, source_sig, target_sig, depth)
        })
    }) || source_shape.construct_signatures.iter().any(|source_sig| {
        target_shape.construct_signatures.iter().any(|target_sig| {
            assertion_signatures_are_comparable(db, source_sig, target_sig, depth)
        })
    })
}

fn is_direct_callable_for_assertion(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(
        db.lookup(type_id),
        Some(TypeData::Function(_) | TypeData::Callable(_))
    )
}

fn assertion_signatures_are_comparable(
    db: &dyn TypeDatabase,
    source: &CallSignature,
    target: &CallSignature,
    depth: u32,
) -> bool {
    let (source_params, source_return) = erase_signature_for_assertion(db, source);
    let (target_params, target_return) = erase_signature_for_assertion(db, target);
    let min_params = source_params.len().min(target_params.len());

    for i in 0..min_params {
        let source_type = comparable_param_type(db, &source_params[i]);
        let target_type = comparable_param_type(db, &target_params[i]);
        if !signature_param_types_are_comparable_for_assertion(db, source_type, target_type, depth)
        {
            return false;
        }
    }

    // An erasure-minted opaque-reference RETURN pair (`Thenable<any>` vs
    // `Promise<any>`: `Application`/`Lazy` whose base this resolver-free
    // query layer cannot materialize) has no extractable properties, so the
    // structural walk below could never find overlap and would sink valid
    // thenable casts into TS2352 (zustand `Thenable<undefined> as
    // Promise<void>`). With every shared-arity parameter pair already
    // comparable, treat such a return pair as overlapping — the same
    // conservative both-opaque policy the Lazy/Lazy rule applies. Scoped to
    // the signature RETURN leg only: an opaque pair in PROPERTY position
    // (`{ p: Promise<void> } as { p: Map<string, number> }`) still decomposes
    // strictly and correctly fails.
    let source_return_is_opaque = matches!(
        db.lookup(source_return),
        Some(TypeData::Application(_) | TypeData::Lazy(_))
    );
    let target_return_is_opaque = matches!(
        db.lookup(target_return),
        Some(TypeData::Application(_) | TypeData::Lazy(_))
    );
    if source_return_is_opaque && target_return_is_opaque {
        return true;
    }

    types_are_comparable_for_assertion_inner(db, source_return, target_return, depth + 1, true)
}

fn signature_param_types_are_comparable_for_assertion(
    db: &dyn TypeDatabase,
    source: TypeId,
    target: TypeId,
    depth: u32,
) -> bool {
    if let (Some(source_members), Some(target_members)) = (
        nullable_callable_union_members(db, source),
        nullable_callable_union_members(db, target),
    ) {
        return source_members.iter().any(|&source_member| {
            target_members.iter().any(|&target_member| {
                types_are_comparable_for_assertion_inner(
                    db,
                    source_member,
                    target_member,
                    depth + 1,
                    true,
                )
            })
        });
    }

    types_are_comparable_for_assertion_inner(db, source, target, depth + 1, true)
}

fn nullable_callable_union_members(db: &dyn TypeDatabase, type_id: TypeId) -> Option<Vec<TypeId>> {
    let members = get_union_members(db, type_id)?;
    let has_nullable = members.iter().any(|member| member.is_nullable());
    if !has_nullable {
        return None;
    }

    let callable_members: Vec<TypeId> = members
        .into_iter()
        .filter(|&member| is_direct_callable_for_assertion(db, member))
        .collect();
    if callable_members.is_empty() {
        None
    } else {
        Some(callable_members)
    }
}

fn erase_signature_for_assertion(
    db: &dyn TypeDatabase,
    sig: &CallSignature,
) -> (Vec<ParamInfo>, TypeId) {
    if sig.type_params.is_empty() {
        return (sig.params.clone(), sig.return_type);
    }

    let mut substitution = TypeSubstitution::new();
    for param in &sig.type_params {
        substitution.insert(param.name, TypeId::ANY);
    }

    let params = sig
        .params
        .iter()
        .map(|param| ParamInfo {
            suppress_display_optional: false,
            name: param.name,
            type_id: instantiate_type(db, param.type_id, &substitution),
            optional: param.optional,
            rest: param.rest,
        })
        .collect();
    let return_type = instantiate_type(db, sig.return_type, &substitution);
    (params, return_type)
}

fn comparable_param_type(db: &dyn TypeDatabase, param: &ParamInfo) -> TypeId {
    if param.rest {
        get_array_element_type(db, param.type_id).unwrap_or(param.type_id)
    } else {
        param.type_id
    }
}

/// Returns true when `type_id` represents an "object-like" type for the
/// purposes of comparing against the `object` primitive in assertion overlap.
/// Object/Array/Tuple/Callable/Intersection all qualify; primitives like
/// `string`, `number`, `null`, `undefined` do not.
fn is_object_like_for_assertion(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(
        db.lookup(type_id),
        Some(
            TypeData::Object(_)
                | TypeData::ObjectWithIndex(_)
                | TypeData::Array(_)
                | TypeData::Tuple(_)
                | TypeData::Callable(_)
                | TypeData::Function(_)
                | TypeData::Intersection(_)
        )
    )
}

/// Returns true when `type_id` is an object type with **no required named
/// members** — the empty object `{}`, an all-optional object (`{ a?: X }`),
/// or a universal index-signature record (`{ [k: string]: V }`, the inline
/// shape of `Record<PropertyKey, unknown>`). Such an object imposes no
/// structural requirement, so it widely overlaps any type-parameter
/// constraint that contains an object-like member, and the constraint-unwrap
/// rule may walk the constraint for it.
///
/// Index signatures are permitted (a record is still requirement-free), unlike
/// the original empty-`{}`-only predicate which excluded them and so reported a
/// false TS2352 on `Record<PropertyKey, unknown> as T extends object` (#14152,
/// remeda `clone.ts`). A required named member (e.g. `B`'s `bar` in
/// `genericTypeAssertions4.ts`) still disqualifies the object, so
/// `B as T extends A` continues to report TS2352.
fn is_object_without_required_members(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    let shape_id = match db.lookup(type_id) {
        Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => shape_id,
        _ => return false,
    };
    let shape = db.object_shape(shape_id);
    shape.properties.iter().all(|p| p.optional)
}

/// Returns true when `keyof_side` is `KeyOf(_)` and `prim_side` is one of
/// the keyof key-space primitives (`string`, `number`, `symbol`) or a
/// literal of those primitives. Used in assertion-overlap to recognize that
/// `keyof T` is structurally a subset of `string | number | symbol`.
fn is_keyof_to_string_number_symbol(
    db: &dyn TypeDatabase,
    keyof_side: TypeId,
    prim_side: TypeId,
) -> bool {
    if !matches!(db.lookup(keyof_side), Some(TypeData::KeyOf(_))) {
        return false;
    }
    if prim_side == TypeId::STRING || prim_side == TypeId::NUMBER || prim_side == TypeId::SYMBOL {
        return true;
    }
    if let Some(TypeData::Literal(lit)) = db.lookup(prim_side) {
        return matches!(
            lit,
            crate::types::LiteralValue::String(_) | crate::types::LiteralValue::Number(_)
        );
    }
    if let Some(TypeData::UniqueSymbol(_)) = db.lookup(prim_side) {
        return true;
    }
    false
}

fn types_are_comparable_inner(
    db: &dyn TypeDatabase,
    source: TypeId,
    target: TypeId,
    depth: u32,
) -> bool {
    if let Some(result) = ComparabilityDepthState::from_depth(depth).fallback_result() {
        return result;
    }

    // Same type is always comparable
    if source == target {
        return true;
    }

    // `never` is comparable to any type (it's the bottom type, subtype of all types).
    // `any` and `unknown` are also comparable to everything.
    if source == TypeId::NEVER
        || target == TypeId::NEVER
        || source == TypeId::ANY
        || target == TypeId::ANY
        || source == TypeId::UNKNOWN
        || target == TypeId::UNKNOWN
    {
        return true;
    }

    // Lazy (unresolved nominal reference like interface/class/enum) cannot be
    // structurally decomposed at the solver level. When encountered during a
    // property-level comparability check (depth > 0), assume the types are
    // comparable — the prior assignability checks already rejected the strict
    // case. At depth 0 the caller should have evaluated/resolved top-level
    // types, so Lazy is unexpected; fall through to other checks.
    if depth > 0
        && (matches!(db.lookup(source), Some(TypeData::Lazy(_)))
            || matches!(db.lookup(target), Some(TypeData::Lazy(_))))
    {
        return true;
    }

    // Unwrap ReadonlyType wrappers — `readonly T[]` is comparable to `T[]`
    if let Some(TypeData::ReadonlyType(inner)) = db.lookup(source) {
        return types_are_comparable_inner(db, inner, target, depth + 1);
    }
    if let Some(TypeData::ReadonlyType(inner)) = db.lookup(target) {
        return types_are_comparable_inner(db, source, inner, depth + 1);
    }

    // Type parameters are not automatically comparable for TS2352 purposes.
    // Treating them as "comparable to anything" suppresses valid diagnostics
    // like asserting a specific subtype to an unconcretized type parameter.

    // Check union types: a union is comparable if ANY member is comparable
    if let Some(TypeData::Union(list_id)) = db.lookup(source) {
        let members = db.type_list(list_id);
        return members
            .iter()
            .any(|&m| types_are_comparable_inner(db, m, target, depth + 1));
    }
    if let Some(TypeData::Union(list_id)) = db.lookup(target) {
        let members = db.type_list(list_id);
        return members
            .iter()
            .any(|&m| types_are_comparable_inner(db, source, m, depth + 1));
    }

    // Array comparability: Array<A> is comparable to Array<B> if A and B are comparable.
    // This handles cases like `(string | number)[]` as `string[]`.
    if let Some(TypeData::Array(source_elem)) = db.lookup(source)
        && let Some(TypeData::Array(target_elem)) = db.lookup(target)
    {
        return types_are_comparable_inner(db, source_elem, target_elem, depth + 1);
    }

    // Tuple→Array comparability: a tuple is comparable to an array if any tuple element
    // type is comparable to the array element type. tsc compares the tuple's element union
    // (number-indexed type) against the array's element type.
    if let Some(TypeData::Tuple(source_tuple_id)) = db.lookup(source)
        && let Some(TypeData::Array(target_elem)) = db.lookup(target)
    {
        let elements = db.tuple_list(source_tuple_id);
        return elements
            .iter()
            .any(|elem| types_are_comparable_inner(db, elem.type_id, target_elem, depth + 1));
    }
    // Array→Tuple comparability: symmetric case.
    if let Some(TypeData::Array(source_elem)) = db.lookup(source)
        && let Some(TypeData::Tuple(target_tuple_id)) = db.lookup(target)
    {
        let elements = db.tuple_list(target_tuple_id);
        return elements
            .iter()
            .any(|elem| types_are_comparable_inner(db, source_elem, elem.type_id, depth + 1));
    }

    // Tuple↔Tuple comparability: two tuples are comparable if corresponding
    // element types are pairwise comparable. For fixed-length tuples, lengths
    // must match. Rest elements are compared by their element type.
    if let Some(TypeData::Tuple(source_tuple_id)) = db.lookup(source)
        && let Some(TypeData::Tuple(target_tuple_id)) = db.lookup(target)
    {
        let source_elems = db.tuple_list(source_tuple_id);
        let target_elems = db.tuple_list(target_tuple_id);

        // Count non-rest elements and find rest elements
        let source_fixed: Vec<_> = source_elems.iter().filter(|e| !e.rest).collect();
        let target_fixed: Vec<_> = target_elems.iter().filter(|e| !e.rest).collect();
        let source_rest = source_elems.iter().find(|e| e.rest);
        let target_rest = target_elems.iter().find(|e| e.rest);

        // For fixed-length tuples (no rest elements), lengths must match
        if source_rest.is_none() && target_rest.is_none() {
            if source_fixed.len() != target_fixed.len() {
                return false;
            }
            return source_fixed
                .iter()
                .zip(target_fixed.iter())
                .all(|(s, t)| types_are_comparable_inner(db, s.type_id, t.type_id, depth + 1));
        }

        // With rest elements, check that the overlapping fixed portion is comparable
        let min_fixed = source_fixed.len().min(target_fixed.len());
        for i in 0..min_fixed {
            if !types_are_comparable_inner(
                db,
                source_fixed[i].type_id,
                target_fixed[i].type_id,
                depth + 1,
            ) {
                return false;
            }
        }
        return true;
    }

    // Callable types: check if their signatures are comparable.
    // Two callable types are comparable if they share comparable call/construct
    // signatures (parameter types and return type all comparable), OR if they
    // share common properties with comparable types.
    if let Some(TypeData::Callable(source_id)) = db.lookup(source)
        && let Some(TypeData::Callable(target_id)) = db.lookup(target)
    {
        let source_shape = db.callable_shape(source_id);
        let target_shape = db.callable_shape(target_id);

        // Check if call signatures are comparable
        if let (Some(s_sig), Some(t_sig)) = (
            source_shape.call_signatures.first(),
            target_shape.call_signatures.first(),
        ) && signatures_are_comparable(db, s_sig, t_sig, depth)
        {
            return true;
        }

        // Check construct signatures
        if let (Some(s_sig), Some(t_sig)) = (
            source_shape.construct_signatures.first(),
            target_shape.construct_signatures.first(),
        ) && signatures_are_comparable(db, s_sig, t_sig, depth)
        {
            return true;
        }

        // Fall through to property overlap check
        return types_have_common_properties(db, source, target, depth);
    }

    // Enum comparability: a literal is comparable to an enum if it matches any enum member.
    // Enums store their member types as a union in the second TypeId field.
    // For string enums, string literals are comparable if they match any member value.
    // For numeric enums, number literals are comparable if they match any member value.
    if let Some(TypeData::Enum(_def_id, members_type_id)) = db.lookup(source) {
        return types_are_comparable_inner(db, members_type_id, target, depth + 1);
    }
    if let Some(TypeData::Enum(_def_id, members_type_id)) = db.lookup(target) {
        return types_are_comparable_inner(db, source, members_type_id, depth + 1);
    }

    // Check primitive ↔ literal comparability
    // string is comparable to any string literal
    // number is comparable to any numeric literal
    // etc.
    if is_primitive_comparable(db, source, target) || is_primitive_comparable(db, target, source) {
        return true;
    }

    // Check object property overlap
    types_have_common_properties(db, source, target, depth)
}

/// Check if two call signatures are comparable: all overlapping parameter pairs
/// and the return types must be comparable.
fn signatures_are_comparable(
    db: &dyn TypeDatabase,
    source: &crate::types::CallSignature,
    target: &crate::types::CallSignature,
    depth: u32,
) -> bool {
    let min_params = source.params.len().min(target.params.len());
    for i in 0..min_params {
        if !types_are_comparable_inner(
            db,
            source.params[i].type_id,
            target.params[i].type_id,
            depth + 1,
        ) {
            return false;
        }
    }
    types_are_comparable_inner(db, source.return_type, target.return_type, depth + 1)
}

/// Whether `type_id` is a template-literal type or a string-intrinsic mapping
/// type (`Uppercase`/`Lowercase`/`Capitalize`/`Uncapitalize`). Both are
/// subtypes of `string` whose membership is pattern-defined.
fn is_template_or_string_intrinsic(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    matches!(
        db.lookup(type_id),
        Some(TypeData::TemplateLiteral(_) | TypeData::StringIntrinsic { .. })
    )
}

/// Whether `type_id` belongs to the `string` domain for assertion overlap:
/// the `string` primitive, a string-literal type, or a pattern-defined string
/// subtype (template-literal / string-intrinsic). A literal source widens to
/// its base primitive at the assertion site, so each of these overlaps a
/// template-literal target.
fn is_string_domain_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    type_id == TypeId::STRING
        || matches!(
            db.lookup(type_id),
            Some(
                TypeData::Literal(crate::types::LiteralValue::String(_))
                    | TypeData::TemplateLiteral(_)
                    | TypeData::StringIntrinsic { .. }
            )
        )
}

/// Check if a base primitive type is comparable to a literal or other form of that primitive.
pub(super) fn is_primitive_comparable(db: &dyn TypeDatabase, base: TypeId, other: TypeId) -> bool {
    // Decompose union types: a union is primitive-comparable if any member is.
    // This is needed for enum structural types which are stored as unions of
    // member literals (e.g., `"" | "time" | "system" | "location"`).
    if let Some(TypeData::Union(list_id)) = db.lookup(base) {
        let members = db.type_list(list_id);
        return members
            .iter()
            .any(|&m| is_primitive_comparable(db, m, other));
    }
    if let Some(TypeData::Union(list_id)) = db.lookup(other) {
        let members = db.type_list(list_id);
        return members
            .iter()
            .any(|&m| is_primitive_comparable(db, base, m));
    }
    // string is comparable to string literals
    if base == TypeId::STRING {
        if let Some(TypeData::Literal(lit)) = db.lookup(other) {
            return matches!(lit, crate::types::LiteralValue::String(_));
        }
        return other == TypeId::STRING;
    }
    // number is comparable to numeric literals
    if base == TypeId::NUMBER {
        if let Some(TypeData::Literal(lit)) = db.lookup(other) {
            return matches!(lit, crate::types::LiteralValue::Number(_));
        }
        return other == TypeId::NUMBER;
    }
    // boolean is comparable to true/false
    if base == TypeId::BOOLEAN {
        return other == TypeId::BOOLEAN_TRUE
            || other == TypeId::BOOLEAN_FALSE
            || other == TypeId::BOOLEAN;
    }
    // bigint is comparable to bigint literals
    if base == TypeId::BIGINT {
        if let Some(TypeData::Literal(lit)) = db.lookup(other) {
            return matches!(lit, crate::types::LiteralValue::BigInt(_));
        }
        return other == TypeId::BIGINT;
    }
    // symbol is comparable to unique symbol (unique symbol is a subtype of symbol)
    if base == TypeId::SYMBOL {
        return matches!(db.lookup(other), Some(TypeData::UniqueSymbol(_)))
            || other == TypeId::SYMBOL;
    }
    // unique symbol is comparable to symbol and to other unique symbols
    if let Some(TypeData::UniqueSymbol(_)) = db.lookup(base) {
        return other == TypeId::SYMBOL
            || matches!(db.lookup(other), Some(TypeData::UniqueSymbol(_)));
    }
    // Two literals of the same primitive kind are broadly comparable. Assertion
    // property overlap applies an additional value-level guard for shared
    // discriminant/phantom properties before reaching this helper.
    if let Some(TypeData::Literal(lit_a)) = db.lookup(base) {
        if let Some(TypeData::Literal(lit_b)) = db.lookup(other) {
            return std::mem::discriminant(&lit_a) == std::mem::discriminant(&lit_b);
        }
        // literal vs its base primitive: "foo" ~ string, 1 ~ number
        return match lit_a {
            crate::types::LiteralValue::String(_) => other == TypeId::STRING,
            crate::types::LiteralValue::Number(_) => other == TypeId::NUMBER,
            crate::types::LiteralValue::BigInt(_) => other == TypeId::BIGINT,
            crate::types::LiteralValue::Boolean(_) => {
                other == TypeId::BOOLEAN
                    || other == TypeId::BOOLEAN_TRUE
                    || other == TypeId::BOOLEAN_FALSE
            }
        };
    }
    // true/false are comparable to each other
    if (base == TypeId::BOOLEAN_TRUE || base == TypeId::BOOLEAN_FALSE)
        && (other == TypeId::BOOLEAN_TRUE || other == TypeId::BOOLEAN_FALSE)
    {
        return true;
    }
    // Enum members are comparable via their underlying structural (literal) type.
    // E.g., `AutomationMode.NONE` (Enum(_, "")) is comparable to `""` and to `string`.
    if let Some(TypeData::Enum(_, structural)) = db.lookup(base) {
        return is_primitive_comparable(db, structural, other)
            || is_primitive_comparable(db, other, structural);
    }
    if let Some(TypeData::Enum(_, structural)) = db.lookup(other) {
        return is_primitive_comparable(db, base, structural)
            || is_primitive_comparable(db, structural, base);
    }
    false
}

/// Check if two types have common properties with ALL of them having comparable types.
///
/// Returns true when the types share at least one property name AND every shared
/// property has comparable types. This matches tsc's behavior for the comparable
/// relation on object types — a single incompatible shared property means the
/// types are NOT comparable, even if other properties match.
fn types_have_common_properties(
    db: &dyn TypeDatabase,
    source: TypeId,
    target: TypeId,
    depth: u32,
) -> bool {
    // Helper to get properties from an object/callable type.
    // Returns (name, type_id, optional) — the optional flag is needed because
    // optional properties implicitly include `undefined` for comparability.
    fn get_properties(db: &dyn TypeDatabase, type_id: TypeId) -> Vec<(Atom, TypeId, bool)> {
        if type_id.is_intrinsic() {
            return Vec::new();
        }
        match db.lookup(type_id) {
            Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) => {
                let shape = db.object_shape(shape_id);
                shape
                    .properties
                    .iter()
                    .map(|p| (p.name, p.type_id, p.optional))
                    .collect()
            }
            Some(TypeData::Callable(callable_id)) => {
                let shape = db.callable_shape(callable_id);
                shape
                    .properties
                    .iter()
                    .map(|p| (p.name, p.type_id, p.optional))
                    .collect()
            }
            Some(TypeData::Intersection(list_id)) => {
                let members = db.type_list(list_id);
                let mut props = Vec::new();
                for &member in members.iter() {
                    props.extend(get_properties(db, member));
                }
                props
            }
            // Arrays have no named properties for overlap checking - element types
            // are compared separately in types_are_comparable_for_assertion_inner.
            // Returning empty ensures we don't short-circuit array↔object comparisons.
            _ => Vec::new(),
        }
    }

    // Handle array↔array comparability: check element types directly
    if let (Some(TypeData::Array(src_elem)), Some(TypeData::Array(tgt_elem))) =
        (db.lookup(source), db.lookup(target))
    {
        return types_are_comparable_for_assertion_inner(db, src_elem, tgt_elem, depth + 1, false);
    }

    // Handle array↔tuple comparability: array element vs any tuple element
    if let (Some(TypeData::Array(arr_elem)), Some(TypeData::Tuple(tuple_id))) =
        (db.lookup(source), db.lookup(target))
    {
        let tuple_elements = db.tuple_list(tuple_id);
        return tuple_elements.iter().any(|elem| {
            types_are_comparable_for_assertion_inner(db, arr_elem, elem.type_id, depth + 1, false)
        });
    }
    if let (Some(TypeData::Tuple(tuple_id)), Some(TypeData::Array(arr_elem))) =
        (db.lookup(source), db.lookup(target))
    {
        let tuple_elements = db.tuple_list(tuple_id);
        return tuple_elements.iter().any(|elem| {
            types_are_comparable_for_assertion_inner(db, elem.type_id, arr_elem, depth + 1, false)
        });
    }

    // Handle tuple↔tuple comparability: check element types pairwise.
    // tsc's isTypeComparableTo checks tuples structurally: each element at
    // position i must be comparable to the element at position i in the other
    // tuple. Different-length tuples are not comparable (neither is assignable
    // to the other), so TS2352 should fire.
    if let (Some(TypeData::Tuple(src_tuple)), Some(TypeData::Tuple(tgt_tuple))) =
        (db.lookup(source), db.lookup(target))
    {
        let src_elements = db.tuple_list(src_tuple);
        let tgt_elements = db.tuple_list(tgt_tuple);
        // Different-length tuples are not comparable
        if src_elements.len() != tgt_elements.len() {
            return false;
        }
        // All corresponding elements must be comparable
        return src_elements.iter().zip(tgt_elements.iter()).all(|(s, t)| {
            types_are_comparable_for_assertion_inner(db, s.type_id, t.type_id, depth + 1, false)
        });
    }

    let source_props = get_properties(db, source);
    let target_props = get_properties(db, target);

    // If both sides have no properties and aren't arrays/tuples, they don't overlap
    if source_props.is_empty() && target_props.is_empty() {
        return false;
    }

    // Build a lookup table for source properties by name.
    use rustc_hash::FxHashMap;
    let mut source_by_name: FxHashMap<Atom, Vec<(TypeId, bool)>> = FxHashMap::default();
    for (name, ty, optional) in &source_props {
        source_by_name
            .entry(*name)
            .or_default()
            .push((*ty, *optional));
    }

    // tsc's comparable relation requires ALL required target properties to
    // exist in the source with comparable types. Just sharing some common
    // property names is not enough — missing required target properties means
    // the types are NOT comparable.
    let mut found_common = false;
    for (target_name, target_ty, target_optional) in &target_props {
        if let Some(source_entries) = source_by_name.get(target_name) {
            found_common = true;
            let any_comparable = source_entries.iter().any(|(source_ty, source_optional)| {
                // An optional property implicitly includes `undefined`
                // (`a?: string` is effectively `string | undefined`), so an
                // `undefined`-typed counterpart overlaps it at `undefined` —
                // but only when BOTH effective types contain `undefined`.
                // A source `issues?: undefined` against a REQUIRED target
                // `issues: [T, ...T[]]` shares no value at all: the source
                // side holds only `undefined` and the target side holds none.
                // tsc rejects that pair ("Type 'undefined' is not comparable
                // to type '[T, ...]'"), which is what fires TS2352 on
                // valibot's `dataset as OutputDataset<...>`; the one-sided
                // form of this grace was the only reason tsz accepted it.
                let source_holds_undefined = *source_ty == TypeId::UNDEFINED || *source_optional;
                let target_holds_undefined = *target_ty == TypeId::UNDEFINED || *target_optional;
                if source_holds_undefined
                    && target_holds_undefined
                    && (*source_ty == TypeId::UNDEFINED || *target_ty == TypeId::UNDEFINED)
                {
                    return true;
                }
                types_are_comparable_inner(db, *source_ty, *target_ty, depth + 1)
            });
            if !any_comparable {
                return false;
            }
        } else if !target_optional {
            // Required target property is missing from source — not comparable.
            return false;
        }
    }
    found_common
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::construction::TypeInterner;

    #[test]
    fn comparability_depth_state_names_cap_boundary() {
        assert_eq!(
            ComparabilityDepthState::from_depth(MAX_COMPARABILITY_DEPTH),
            ComparabilityDepthState::WithinLimit
        );
        assert_eq!(
            ComparabilityDepthState::from_depth(MAX_COMPARABILITY_DEPTH + 1),
            ComparabilityDepthState::LimitExceeded
        );
        assert_eq!(
            ComparabilityDepthState::LimitExceeded.fallback_result(),
            Some(false)
        );
    }

    #[test]
    fn comparability_depth_state_preserves_strict_and_assertion_fallback() {
        let interner = TypeInterner::new();

        assert!(types_are_comparable_inner(
            &interner,
            TypeId::STRING,
            TypeId::STRING,
            MAX_COMPARABILITY_DEPTH
        ));
        assert!(!types_are_comparable_inner(
            &interner,
            TypeId::STRING,
            TypeId::STRING,
            MAX_COMPARABILITY_DEPTH + 1
        ));
        assert!(types_are_comparable_for_assertion_inner(
            &interner,
            TypeId::STRING,
            TypeId::STRING,
            MAX_COMPARABILITY_DEPTH,
            false
        ));
        assert!(!types_are_comparable_for_assertion_inner(
            &interner,
            TypeId::STRING,
            TypeId::STRING,
            MAX_COMPARABILITY_DEPTH + 1,
            false
        ));
    }
}
