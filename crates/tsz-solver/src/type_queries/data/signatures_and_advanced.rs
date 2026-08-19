//! Type parameter queries, signature helpers, function rewrites,
//! conditional/mapped type accessors, literal property key collection,
//! impossible-member pruning, private brand/field queries, enum helpers,
//! and base-type validity checks.

use super::accessors::get_object_shape;
use super::content_predicates::{
    contains_infer_types_db, contains_type_parameters_db, get_intersection_members,
};
use crate::construction::{QueryDatabase, TypeDatabase};
use crate::evaluation::evaluate::{evaluate_index_access, evaluate_type};
use crate::instantiation::instantiate::instantiate_type_params_to_constraints_uncached;
use crate::types::{CallSignature, ConditionalType, TypeData, TypeId};
use rustc_hash::{FxHashMap, FxHashSet};
use tsz_common::Atom;

/// Get the type parameter info if this is a type parameter.
///
/// Returns None if not a type parameter.
pub fn get_type_parameter_info(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<crate::types::TypeParamInfo> {
    if type_id.is_intrinsic() {
        return None;
    }
    match db.lookup(type_id) {
        Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => Some(info),
        _ => None,
    }
}

/// Check if a type is a type parameter (`TypeParameter` or Infer).
pub fn is_type_parameter(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    matches!(
        db.lookup(type_id),
        Some(TypeData::TypeParameter(_) | TypeData::Infer(_))
    )
}

/// Check if a type is or contains a const type variable.
///
/// Matches tsc's `isConstTypeVariable`: returns true when the type is a type
/// parameter with the `const` modifier, or a union/intersection containing one.
/// This is used to trigger const-like inference (tuple inference for array
/// literals, readonly properties for object literals, literal preservation).
pub fn is_const_type_variable(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if type_id.is_intrinsic() {
        return false;
    }
    match db.lookup(type_id) {
        Some(TypeData::TypeParameter(info)) => info.is_const,
        Some(TypeData::Union(list_id) | TypeData::Intersection(list_id)) => {
            let members = db.type_list(list_id);
            members.iter().any(|&m| is_const_type_variable(db, m))
        }
        _ => false,
    }
}

/// Get the constraint of a type parameter.
///
/// Returns None if not a type parameter or has no constraint.
pub fn get_type_parameter_constraint(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    if type_id.is_intrinsic() {
        return None;
    }
    match db.lookup(type_id) {
        Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => info.constraint,
        _ => None,
    }
}

/// Get the interned name of a type parameter.
///
/// Returns `Some(Atom)` for `TypeParameter` and `Infer` types, `None` otherwise.
pub fn get_type_parameter_name(db: &dyn TypeDatabase, type_id: TypeId) -> Option<Atom> {
    if type_id.is_intrinsic() {
        return None;
    }
    match db.lookup(type_id) {
        Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => Some(info.name),
        _ => None,
    }
}

/// Resolve a type parameter to its base constraint for TS2344 checking.
///
/// If the type IS a `TypeParameter` with a constraint, returns the constraint.
/// If it IS a `TypeParameter` without a constraint, returns `unknown`.
/// Returns the type unchanged for anything else (including `Infer` types,
/// composite types, etc.).
///
/// This is used for TS2344 constraint checking: when a type parameter `U extends number`
/// is used as `T extends string`, tsc resolves `U` to `number` and checks `number <: string`.
/// `Infer` types inside conditional types should NOT be resolved here — they are checked
/// during conditional type evaluation, not at type argument validation time.
pub fn get_base_constraint_of_type(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    // Fast path: intrinsics aren't `TypeParameter(_)`; return as-is.
    if type_id.is_intrinsic() {
        return type_id;
    }
    match db.lookup(type_id) {
        Some(TypeData::TypeParameter(info)) => info.constraint.unwrap_or(TypeId::UNKNOWN),
        Some(TypeData::Substitution {
            base_type,
            constraint,
        }) => {
            // The base constraint of a substitution is the base constraint of
            // its substitution intersection `base & constraint` (tsc:
            // `getBaseConstraint(getSubstitutionIntersection(type))`).
            let base_constraint = get_base_constraint_of_type(db, base_type);
            db.intersection2(base_constraint, constraint)
        }
        _ => type_id,
    }
}

/// Base constraint of an instantiable indexed access `Obj[Idx]`, for the
/// comparability/overlap relation ONLY.
///
/// This is a deliberately narrow reducer kept OUT of the shared
/// [`get_base_constraint_of_type`]: the shared base-constraint query is on the
/// hot path of assignment narrowing, generic-call normalization, and type-arg
/// constraint validation, where reducing an instantiable indexed access changes
/// the displayed/relation surface (e.g. it strips the `| undefined` that tsc
/// keeps on `Partial<T>[keyof T]` in an assignment diagnostic). The comparability
/// relation (TS2678 switch/case, TS2367 `===`/`!==`) is the only caller that
/// needs tsc's `getReducedApparentType` indexed-access reduction.
///
/// Reduces the object's contained type parameters to their constraints (tsc's
/// base-constraint mapper), evaluates that, then evaluates the indexed access
/// against the reduced object. Returns the input `type_id` unchanged when it is
/// not an `IndexAccess`, when no reduction is possible (the index access remains
/// genuinely deferred), or when reduction produces an `Error`, so the relation
/// sees the same opaque type it would have before.
///
/// For example `Parameters<F>["length"]` where `F extends (...args: any[]) => any`
/// reduces to `any[]["length"]` = `number`, so a numeric-literal switch/`===`
/// operand can overlap it (no false TS2678/TS2367).
pub fn reduce_index_access_to_base_constraint(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    if type_id.is_intrinsic() {
        return type_id;
    }
    let Some(TypeData::IndexAccess(object, index)) = db.lookup(type_id) else {
        return type_id;
    };

    // Reduce the object's type parameters to their constraints, then evaluate so
    // an alias `Application` (e.g. `Parameters<(...args: any[]) => any>`)
    // collapses to its concrete body (`any[]`).
    let object_constraint = evaluate_type(
        db,
        instantiate_type_params_to_constraints_uncached(db, object),
    );

    // No progress reducing the object: keep the deferred form.
    if object_constraint == object {
        return type_id;
    }

    let resolved = evaluate_index_access(db, object_constraint, index);
    if resolved == TypeId::ERROR || resolved == type_id {
        type_id
    } else {
        resolved
    }
}

/// Ordered constraint-walk display steps for a deferred, constraint-relative
/// source in a failed nullable-union assignability elaboration, mirroring
/// `tsc`'s `reportRelationError` recursion through `getConstraintOfType`.
///
/// `tsc` renders the as-written operand first (`TBox[KKey]`), then walks its
/// constraint one step at a time — substituting a type-parameter index with
/// its declared constraint (`TBox[keyof TBox]`), expanding a deferred `keyof`
/// to its `string | number | symbol` base key space and distributing the index
/// union (`TBox[string] | TBox[number] | TBox[symbol]`), and finally drilling a
/// distributed union source to its first member not among the target's real
/// members (`TBox[string]`). A concrete object short-circuits the entire walk
/// to the resolved value type in a single step (`Obj[KP]` -> `number`).
///
/// Returns the steps STRICTLY AFTER `source` (each distinct from those already
/// emitted), terminating at the leaf. The caller renders `source` itself; this
/// supplies the elaboration lines beneath it. Empty when `source` has no
/// further constraint to display.
///
/// `target` only selects the first failing member when a step distributes to a
/// union (its nullish-stripped real members are the ones a source member may
/// legitimately match). This is display-only: the assignability relation itself
/// always runs against the full, unwalked types.
pub fn indexed_access_constraint_display_walk(
    db: &dyn TypeDatabase,
    source: TypeId,
    target: TypeId,
) -> Vec<ConstraintWalkStep> {
    let mut steps: Vec<ConstraintWalkStep> = Vec::new();
    let mut cur = source;
    // A deferred constraint walk is short (index -> keyof -> distribute ->
    // member is the deepest `tsc` emits); the cap is a non-termination backstop.
    for _ in 0..8 {
        let Some(next) = constraint_walk_step(db, cur, target) else {
            break;
        };
        if next == source || steps.iter().any(|s| s.type_id == next) {
            break;
        }
        // A concrete step (an intrinsic, or a concrete-base access the caller's
        // resolver renders as its value type, e.g. `Obj[keyof Obj]` -> `number`)
        // collapses the nullable target to its single real member and terminates
        // the walk; a generic-base access or a distributed union keeps the full
        // union and the walk continues.
        let concrete = is_walk_terminal(db, next);
        steps.push(ConstraintWalkStep {
            type_id: next,
            concrete,
        });
        if concrete {
            break;
        }
        cur = next;
    }
    steps
}

/// One constraint-walk display step: the intermediate type `tsc` renders, and
/// whether it is a concrete leaf (which collapses a nullable target to its
/// single real member) versus a still-deferred generic form (which keeps the
/// full union).
#[derive(Debug, Clone, Copy)]
pub struct ConstraintWalkStep {
    pub type_id: TypeId,
    pub concrete: bool,
}

/// Whether a walk step is the terminal leaf: an intrinsic, a concrete-base
/// indexed access (which the caller's resolver reduces to its value type), or
/// any other non-instantiable type. A generic-base access or a distributed
/// union is NOT terminal — the walk continues through its constraint / members.
fn is_walk_terminal(db: &dyn TypeDatabase, ty: TypeId) -> bool {
    if ty.is_intrinsic() {
        return true;
    }
    match db.lookup(ty) {
        Some(TypeData::IndexAccess(obj, _)) => {
            !matches!(db.lookup(obj), Some(TypeData::TypeParameter(_)))
        }
        Some(TypeData::Union(_) | TypeData::KeyOf(_)) => false,
        _ => true,
    }
}

/// One constraint-walk step from `cur`: drill a distributed union to its first
/// failing member, expand a bare `keyof`, or walk an indexed access's
/// index/object constraint. `None` when `cur` has no further step.
fn constraint_walk_step(db: &dyn TypeDatabase, cur: TypeId, target: TypeId) -> Option<TypeId> {
    if cur.is_intrinsic() {
        return None;
    }
    match db.lookup(cur)? {
        // A distributed (or intrinsic-key) union source drills to its first
        // member that is not among the target's real (nullish-stripped)
        // members — `tsc` reports the first union constituent that fails.
        TypeData::Union(list) => {
            let members = db.type_list(list);
            let reals = nullish_real_members(db, target);
            members
                .iter()
                .copied()
                .find(|m| !reals.contains(m))
                .or_else(|| members.first().copied())
        }
        // An indexed access walks its index (then its object) constraint. A
        // bare `keyof X` source is deliberately NOT expanded here: its base key
        // space `string | number | symbol` renders through the `PropertyKey`
        // display alias, which does not match `tsc`'s expanded form — that
        // surface is left to a separate printer-alias fix. Inside an indexed
        // access the same key space is distributed per-member
        // (`T[string] | T[number] | T[symbol]`), which never reaches the alias.
        TypeData::IndexAccess(obj, idx) => index_access_walk_step(db, obj, idx),
        _ => None,
    }
}

/// `getConstraintFromIndexedAccess`: simplify the index one step, else the
/// object, building the resulting access the way `getIndexedAccessType` would.
fn index_access_walk_step(db: &dyn TypeDatabase, obj: TypeId, idx: TypeId) -> Option<TypeId> {
    if let Some(index_constraint) = one_step_constraint(db, idx)
        && index_constraint != idx
    {
        return Some(build_index_access_step(db, obj, index_constraint));
    }
    if let Some(object_constraint) = one_step_constraint(db, obj)
        && object_constraint != obj
    {
        let access = evaluate_index_access(db, object_constraint, idx);
        // The object's constraint has no member at this index (`{ a: number }`
        // has no `[string]`), so `tsc`'s `getConstraintFromIndexedAccess`
        // returns nothing and the walk stops — do not leak the `any`/error
        // that a missing member evaluates to as a spurious walk line.
        if !matches!(
            access,
            TypeId::ANY | TypeId::ERROR | TypeId::UNKNOWN | TypeId::NEVER
        ) && access != obj
        {
            return Some(access);
        }
    }
    None
}

/// One step of `getConstraintOfType` for the index/object of a deferred access:
/// a type parameter yields its declared constraint; a deferred `keyof` yields
/// its `string | number | symbol` base key space. Everything else has no
/// further single-step constraint here.
fn one_step_constraint(db: &dyn TypeDatabase, ty: TypeId) -> Option<TypeId> {
    get_type_parameter_constraint(db, ty).or_else(|| {
        matches!(db.lookup(ty), Some(TypeData::KeyOf(_)))
            .then(|| db.union3(TypeId::STRING, TypeId::NUMBER, TypeId::SYMBOL))
    })
}

/// Build `obj[index_constraint]` the way `tsc`'s `getIndexedAccessType` would
/// for display: a generic (type-parameter) object stays deferred and
/// distributes a union index into a union of accesses; a concrete object
/// resolves the access to its value type.
fn build_index_access_step(db: &dyn TypeDatabase, obj: TypeId, index_constraint: TypeId) -> TypeId {
    let obj_is_generic = matches!(db.lookup(obj), Some(TypeData::TypeParameter(_)));
    if obj_is_generic {
        if let Some(TypeData::Union(list)) = db.lookup(index_constraint) {
            let accesses: Vec<TypeId> = db
                .type_list(list)
                .iter()
                .map(|&m| db.index_access(obj, m))
                .collect();
            return db.union(accesses);
        }
        return db.index_access(obj, index_constraint);
    }
    // Concrete object: resolve the access to its value type. `evaluate_type`
    // over the built access simplifies both a deferred `keyof` index and the
    // resulting member lookup (`Obj[keyof Obj]` -> `number`), where the
    // narrower `evaluate_index_access` would leave the access deferred when the
    // index still needs its own evaluation.
    evaluate_type(db, db.index_access(obj, index_constraint))
}

/// The real (non-`null`/`undefined`) members of a possibly-nullable target, or
/// the target itself when it is not a union.
fn nullish_real_members(db: &dyn TypeDatabase, target: TypeId) -> Vec<TypeId> {
    match db.lookup(target) {
        Some(TypeData::Union(list)) => db
            .type_list(list)
            .iter()
            .copied()
            .filter(|&m| m != TypeId::NULL && m != TypeId::UNDEFINED)
            .collect(),
        _ => vec![target],
    }
}

/// Compute the default constraint of a deferred conditional type, mirroring
/// tsc's `getDefaultConstraintOfConditionalType`: the union of the inferred
/// true-branch type and the false-branch type.
///
/// For `T extends U ? X : Y`, the inferred true type is `X` with the check type
/// narrowed to `T & U` (handled here for the common Extract-style patterns where
/// the true branch is the check type or a nested conditional over the same check
/// type); the default constraint is then `inferredTrue | Y`. When either branch
/// is `any`, only the other branch is returned so the constraint does not
/// collapse to `any` (since `X | any = any`).
///
/// Returns `None` when `type_id` is not a `Conditional`, or when neither the
/// check type nor the extends type contains type parameters (the conditional is
/// not deferred — the evaluator would already have selected a branch).
///
/// This is the apparent type of a deferred conditional (tsc's `getApparentType`
/// resolves a conditional through this constraint), so it is the key space used
/// to validate an indexed access into, or a comparability/assertion against, a
/// deferred conditional.
pub fn get_conditional_default_constraint(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<TypeId> {
    let cond_id = crate::type_queries::get_conditional_type_id(db, type_id)?;
    let cond = db.conditional_type(cond_id);
    conditional_default_constraint_from_data(db, &cond)
}

/// [`get_conditional_default_constraint`] for an already-resolved
/// [`ConditionalType`]. Shared by the subtype relation's constraint reduction.
pub fn conditional_default_constraint_from_data(
    db: &dyn TypeDatabase,
    cond: &ConditionalType,
) -> Option<TypeId> {
    let is_check_type_param =
        matches!(db.lookup(cond.check_type), Some(TypeData::TypeParameter(_)));
    let check_has_params = is_check_type_param || contains_type_parameters_db(db, cond.check_type);

    // If neither operand contains type parameters, the evaluator would have
    // already picked a branch — there is no deferred constraint to compute.
    // Only walk `extends_type` when the check side did not already qualify.
    if !check_has_params && !contains_type_parameters_db(db, cond.extends_type) {
        return None;
    }

    // Inferred true type: the true branch with the check type narrowed to
    // `check_type & extends_type`. We handle the Extract-style patterns that do
    // not require full instantiation:
    //   - `T extends U ? T : Y`              -> inferred true = T & U
    //   - `T extends U ? (T extends V ? T : never) : never` (nested) -> T & U & V
    // Otherwise the true branch is used unchanged (its key space is unaffected
    // by narrowing the check parameter).
    let inferred_true = if cond.true_type == cond.check_type {
        db.intersection2(cond.check_type, cond.extends_type)
    } else if is_check_type_param {
        match nested_conditional_default_constraint(db, cond.true_type, cond.check_type) {
            Some(inner) => db.intersection2(inner, cond.extends_type),
            None => cond.true_type,
        }
    } else {
        cond.true_type
    };

    let constraint = if inferred_true == TypeId::ANY || cond.false_type == TypeId::ANY {
        inferred_true
    } else {
        db.union2(inferred_true, cond.false_type)
    };
    Some(constraint)
}

/// Apparent **value** type of a deferred conditional, for spread / iteration
/// element extraction.
///
/// Mirrors tsc's `getApparentType` -> `getDefaultConstraintOfConditionalType`,
/// where `getInferredTrueTypeFromConditionalType` narrows the check type
/// parameter to `check & extends` throughout the *whole* true branch. Unlike
/// [`get_conditional_default_constraint`] — which narrows only when the true
/// branch **is** the check type (enough for indexed-access key-space
/// validation, where a wrapped occurrence such as `[T]` keeps the same key
/// space) — this instantiates the entire true branch with `check := check &
/// extends`, so a wrapped occurrence carries the narrowed element type.
///
/// For `v: T extends U ? [T] : Y`, the iterated element of the true branch is
/// then `T & U` (assignable to `U`) rather than the unconstrained `T`, which is
/// what a spread `...v` must relate to the rest-parameter element. The false
/// branch is used unchanged, mirroring tsc. When one branch is `any`, tsc keeps
/// the other branch (a union with `any` would otherwise erase the constraint).
///
/// Returns `None` when `type_id` is not a deferred conditional, or when the
/// constraint makes no progress (so the caller keeps the type deferred).
pub fn get_conditional_apparent_value_constraint(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<TypeId> {
    let cond_id = crate::type_queries::get_conditional_type_id(db, type_id)?;
    let cond = db.conditional_type(cond_id);

    let check_type_param = match db.lookup(cond.check_type) {
        Some(TypeData::TypeParameter(info)) => Some(info),
        _ => None,
    };

    // Deferred-ness guard: if neither operand carries type parameters the
    // evaluator would already have selected a branch, so there is nothing to
    // reduce here.
    if check_type_param.is_none()
        && !contains_type_parameters_db(db, cond.check_type)
        && !contains_type_parameters_db(db, cond.extends_type)
    {
        return None;
    }

    // Inferred true branch: instantiate with `check := check & extends` so a
    // wrapped occurrence (`[T]`, `T[]`, an alias over `T`, ...) carries the
    // narrowed element type. A branch that does not mention the check parameter
    // (e.g. a concrete `number[]`) is returned unchanged by the instantiator.
    let inferred_true = if let Some(info) = check_type_param {
        let narrowed = db.intersection2(cond.check_type, cond.extends_type);
        let subst =
            crate::instantiation::instantiate::TypeSubstitution::single(info.name, narrowed);
        crate::instantiation::instantiate::instantiate_type(db, cond.true_type, &subst)
    } else {
        cond.true_type
    };

    // Union the branches, but collapse to the non-`any` branch when one is
    // `any` (tsc's `getDefaultConstraintOfConditionalType`): a `X | any` union
    // would otherwise erase the constraint to `any`.
    let constraint = if inferred_true == TypeId::ANY {
        cond.false_type
    } else if cond.false_type == TypeId::ANY {
        inferred_true
    } else {
        db.union2(inferred_true, cond.false_type)
    };
    (constraint != type_id).then_some(constraint)
}

/// Substitute a deferred conditional's check-type parameter with its own base
/// constraint, returning the substituted (still unevaluated) conditional.
///
/// This is the construction half of tsc's `getConstraintFromConditionalType`:
/// for `T extends U ? X : Y` where the check type `T` is a type parameter with
/// a constraint `C` (`C != T`), substituting `T -> C` makes the check type
/// concrete so a subsequent (resolver-backed) evaluation can match the `extends`
/// pattern and select a branch. For a deferred utility such as `Parameters<F>`
/// (whose conditional is `F extends (...args: infer P) => any ? P : never`),
/// the substituted conditional `AnyFunction extends (...args: infer P) => any ?
/// P : never` evaluates to the concrete apparent base `never[]` — which the
/// shallow [`get_base_constraint_of_type`] and the branch-union
/// [`get_conditional_default_constraint`] both leave as an unresolved infer
/// placeholder.
///
/// Returns `None` when `type_id` is not a deferred conditional, when the check
/// type is not a constrained type parameter, or when the substitution is a
/// no-op. The caller is responsible for evaluating the result (resolver-backed
/// when the constraint references imported aliases) and for discarding a `never`
/// collapse in favor of the branch-union default constraint.
pub fn conditional_check_type_substituted_constraint(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<TypeId> {
    let cond_id = crate::type_queries::get_conditional_type_id(db, type_id)?;
    let cond = db.conditional_type(cond_id);
    let constraint = get_base_constraint_of_type(db, cond.check_type);
    conditional_check_type_substituted_with_constraint(db, type_id, constraint)
}

/// Substitute an explicitly exposed constraint for a deferred conditional's
/// naked check parameter.
///
/// This is the resolver-aware companion to
/// [`conditional_check_type_substituted_constraint`]. Callers that own a
/// resolver can first expose an alias/reference constraint, then use this
/// construction helper so the instantiator sees the resulting union and keeps
/// distributive conditional semantics. The supplied constraint is used only
/// for the check parameter owned by `type_id`; evaluation remains the caller's
/// responsibility.
pub fn conditional_check_type_substituted_with_constraint(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    constraint: TypeId,
) -> Option<TypeId> {
    let cond_id = crate::type_queries::get_conditional_type_id(db, type_id)?;
    let cond = db.conditional_type(cond_id);
    let TypeData::TypeParameter(info) = db.lookup(cond.check_type)? else {
        return None;
    };
    if constraint == cond.check_type || constraint == TypeId::ERROR {
        return None;
    }
    let subst = crate::instantiation::instantiate::TypeSubstitution::single(info.name, constraint);
    let substituted = crate::instantiation::instantiate::instantiate_type(db, type_id, &subst);
    (substituted != type_id).then_some(substituted)
}

/// Default constraint of a nested conditional whose check type matches
/// `outer_check_type`, recursing for arbitrary Extract-chain depth.
fn nested_conditional_default_constraint(
    db: &dyn TypeDatabase,
    ty: TypeId,
    outer_check_type: TypeId,
) -> Option<TypeId> {
    if ty.is_intrinsic() {
        return None;
    }
    if let Some(TypeData::Conditional(inner_cond_id)) = db.lookup(ty) {
        let inner = db.conditional_type(inner_cond_id);
        if inner.check_type == outer_check_type {
            return conditional_default_constraint_from_data(db, &inner);
        }
    }
    None
}

/// Base constraint of a deferred conditional type, computed as the union of its
/// two branch results (`getBaseConstraintOfType` of a conditional in tsc, which
/// is `getUnionType([trueType, falseType])`).
///
/// This is the apparent type used to validate an index-access key or an
/// assertion source against a *deferred* conditional whose check type still
/// contains unresolved type parameters — e.g. `C<T>['x']` where
/// `C<T> = T extends string ? { x: 1 } : { x: 3 }` validates `'x'` against
/// `keyof ({ x: 1 } | { x: 3 })`. The conditional itself stays deferred so a
/// later concrete instantiation still resolves to the selected branch; only the
/// key/overlap *validation* uses this union.
///
/// A branch that is itself a *deferred conditional* (e.g. a recursive
/// `ParsePathParams<Rest>` chained through `extends` clauses) is flattened into
/// its own branch results, bounded by a small depth/fuel guard so a recursive
/// conditional alias cannot re-enter unboundedly. Only the key/value *domain*
/// is collected this way — branches are not distributed into instantiations.
///
/// Returns `None` when the type is not a `Conditional`, or when a branch is a
/// bare type parameter / `infer` placeholder (tsc keeps those branches'
/// constraints opaque, so a union-of-branches check would be unreliable — mirror
/// the property-access deferral in `operations::property`).
pub fn conditional_branch_union_constraint(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<TypeId> {
    fn collect(
        db: &dyn TypeDatabase,
        type_id: TypeId,
        out: &mut Vec<TypeId>,
        depth: u8,
    ) -> Option<()> {
        let cond = get_conditional_type(db, type_id)?;
        let is_bare_param = |t: TypeId| {
            matches!(
                db.lookup(t),
                Some(TypeData::TypeParameter(_) | TypeData::Infer(_))
            )
        };
        if is_bare_param(cond.true_type) || is_bare_param(cond.false_type) {
            return None;
        }
        for branch in [cond.true_type, cond.false_type] {
            // Descend into a nested conditional branch (recursive utility types),
            // bounded by the depth/fuel guard. Self-referential branches that
            // re-intern to the outer conditional contribute nothing new.
            if depth < 8
                && branch != type_id
                && get_conditional_type(db, branch).is_some()
                && collect(db, branch, out, depth + 1).is_some()
            {
                continue;
            }
            out.push(branch);
        }
        Some(())
    }

    let mut branches = Vec::new();
    collect(db, type_id, &mut branches, 0)?;
    if branches.is_empty() {
        return None;
    }
    let union = db.union(branches);
    // No progress (e.g. one branch is the conditional itself) — keep deferred.
    if union == type_id {
        return None;
    }
    Some(union)
}

/// Constraint of a *distributive* conditional type, mirroring tsc's
/// `getConstraintOfDistributiveConditionalType`.
///
/// For `T extends E ? X : Y` where the check type `T` is a constrained type
/// parameter that does not occur in its own constraint, instantiate the whole
/// conditional with `T := constraint` and evaluate it. This is the constraint
/// tsc reads for such a reference — e.g. the parameter tuple of `Parameters<F>`
/// (`F extends (...a: infer P) => any ? P : never`) resolves, via
/// `F := <F's constraint>`, to the constraint's own parameter list (so
/// `Parameters<F>` where `F extends (...p: never[]) => unknown` resolves to
/// `never[]`).
///
/// Returns `None` when the conditional is not distributive, the check type is
/// not a constrained type parameter, the constraint refers back to the
/// parameter, the instantiation makes no progress, or the evaluated result is
/// `never` — mirroring tsc's `!(instantiated.flags & Never)` guard, which keeps
/// a `never` distributive constraint from being used (callers fall back to the
/// default constraint instead).
pub fn get_distributive_conditional_constraint(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<TypeId> {
    let cond_id = crate::type_queries::get_conditional_type_id(db, type_id)?;
    let cond = db.conditional_type(cond_id);
    if !cond.is_distributive {
        return None;
    }
    let param_info = crate::visitor::type_param_info(db, cond.check_type)?;
    let constraint = param_info.constraint?;
    if crate::visitor::contains_type_parameter_named(db, constraint, param_info.name) {
        return None;
    }
    use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};
    let sub = TypeSubstitution::single(param_info.name, constraint);
    let instantiated = instantiate_type(db, type_id, &sub);
    if instantiated == type_id {
        return None;
    }
    let evaluated = crate::evaluation::evaluate::evaluate_type(db, instantiated);
    if evaluated == type_id || evaluated == TypeId::NEVER {
        return None;
    }
    Some(evaluated)
}

/// Resolve a type to its base constraint for display purposes, recursively reducing
/// type parameters inside unions and intersections.
///
/// This mirrors tsc's `getBaseConstraintOfType` for instantiable types, which for
/// unions/intersections recursively reduces each member and then re-intersects/unions.
/// The intersection of union constraints is simplified via the interner's normal
/// distribution rules (e.g., `(A | B) & (A | C)` reduces to `A | (B & C)` and
/// disjoint primitives collapse to `never`).
///
/// Returns the reduced type, or `type_id` unchanged when there is no simplification.
///
/// Example: for `T & U` where `T extends string | number | undefined` and
/// `U extends string | null | undefined`, this returns `string | undefined`
/// (matching tsc's getBaseConstraintOfType(T & U) output).
pub fn get_base_constraint_for_display(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    fn go(db: &dyn TypeDatabase, type_id: TypeId, depth: u8) -> Option<TypeId> {
        if depth > 6 {
            return None;
        }
        match db.lookup(type_id)? {
            TypeData::TypeParameter(info) => {
                let constraint = info.constraint?;
                // Recursively reduce the constraint to bottom out at a concrete type.
                Some(go(db, constraint, depth + 1).unwrap_or(constraint))
            }
            TypeData::Intersection(list_id) => {
                let members = db.type_list(list_id);
                let mut reduced: Vec<TypeId> = Vec::with_capacity(members.len());
                let mut changed = false;
                for &m in members.iter() {
                    match go(db, m, depth + 1) {
                        Some(r) => {
                            if r != m {
                                changed = true;
                            }
                            reduced.push(r);
                        }
                        None => reduced.push(m),
                    }
                }
                if changed {
                    Some(db.intersection(reduced))
                } else {
                    None
                }
            }
            TypeData::Union(list_id) => {
                let members = db.type_list(list_id);
                let mut reduced: Vec<TypeId> = Vec::with_capacity(members.len());
                let mut changed = false;
                for &m in members.iter() {
                    match go(db, m, depth + 1) {
                        Some(r) => {
                            if r != m {
                                changed = true;
                            }
                            reduced.push(r);
                        }
                        None => reduced.push(m),
                    }
                }
                if changed {
                    Some(db.union(reduced))
                } else {
                    None
                }
            }
            _ => None,
        }
    }
    go(db, type_id, 0).unwrap_or(type_id)
}

/// Compute the "constituent count" of a type for relation complexity estimation.
///
/// Mirrors tsc's `getConstituentCount` used to detect TS2859 before
/// performing expensive structural comparisons:
/// - Union: sum of constituent counts of all members (additive)
/// - Intersection: product of constituent counts of all members (multiplicative)
/// - Everything else: 1
///
/// The caller compares `source_count * target_count` against a threshold
/// (tsc uses 1,000,000) to decide if the comparison is too complex.
pub fn constituent_count(db: &dyn TypeDatabase, type_id: TypeId) -> u64 {
    // Fast path: intrinsics aren't `Union(_)` / `Intersection(_)`; count is 1.
    if type_id.is_intrinsic() {
        return 1;
    }
    match db.lookup(type_id) {
        Some(TypeData::Union(members_id)) => {
            let members = db.type_list(members_id);
            members
                .iter()
                .map(|m| constituent_count(db, *m))
                .sum::<u64>()
                .max(1)
        }
        Some(TypeData::Intersection(members_id)) => {
            let members = db.type_list(members_id);
            members
                .iter()
                .map(|m| constituent_count(db, *m))
                .fold(1u64, |acc, count| acc.saturating_mul(count))
                .max(1)
        }
        _ => 1,
    }
}

/// Get the callable shape for a callable type.
///
/// Returns None if the type is not a Callable.
pub fn get_callable_shape(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<crate::types::CallableShape>> {
    if type_id.is_intrinsic() {
        return None;
    }
    match db.lookup(type_id) {
        Some(TypeData::Callable(shape_id)) => Some(db.callable_shape(shape_id)),
        _ => None,
    }
}

/// Get call signatures from a type.
///
/// For `Callable` types, returns their call signatures directly.
/// For intersection types, collects call signatures from all callable members.
/// Returns None if no call signatures are found.
pub fn get_call_signatures(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<Vec<crate::CallSignature>> {
    if let Some(shape) = get_callable_shape(db, type_id) {
        return Some(shape.call_signatures.clone());
    }
    // For intersection types, collect call signatures from all members
    if let Some(members) = get_intersection_members(db, type_id) {
        let mut all_sigs = Vec::new();
        for member in members.iter() {
            if let Some(shape) = get_callable_shape(db, *member) {
                all_sigs.extend(shape.call_signatures.iter().cloned());
            }
        }
        if !all_sigs.is_empty() {
            return Some(all_sigs);
        }
    }
    None
}

/// Get construct signatures from a type.
///
/// For `Callable` types, returns their construct signatures directly.
/// For intersection types, collects construct signatures from all callable members.
/// Returns None if no construct signatures are found.
pub fn get_construct_signatures(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<Vec<crate::CallSignature>> {
    if let Some(shape) = get_callable_shape(db, type_id) {
        return Some(shape.construct_signatures.clone());
    }
    // For intersection types, collect construct signatures from all members
    if let Some(members) = get_intersection_members(db, type_id) {
        let mut all_sigs = Vec::new();
        for member in members.iter() {
            if let Some(shape) = get_callable_shape(db, *member) {
                all_sigs.extend(shape.construct_signatures.iter().cloned());
            }
        }
        if !all_sigs.is_empty() {
            return Some(all_sigs);
        }
    }
    None
}

/// Returns `true` when the *apparent* type of `type_id` carries a call or
/// construct signature.
///
/// Mirrors tsc's `typeHasCallOrConstructSignatures(getApparentType(type))`:
/// - the apparent type of a type parameter is the apparent type of its base
///   constraint;
/// - `readonly` wrappers are transparent;
/// - an intersection qualifies when *any* member does;
/// - a union qualifies only when *every* member does (an empty union never
///   qualifies);
/// - deferred forms (`Lazy`, `typeof` queries, generic class constructors, and
///   anything that only exposes its constructor/call shape after evaluation)
///   are resolved on demand, guarded against cycles.
///
/// This is the structural predicate behind `instanceof` right-operand
/// eligibility (TS2359): it is intentionally broader than a bare
/// `Callable`/`Function` shape check so a constructor-constrained type
/// parameter or a generic class value is recognised as a constructor without
/// depending on structural assignability to the global `Function` interface.
pub fn apparent_type_has_call_or_construct_signatures(
    db: &dyn QueryDatabase,
    type_id: TypeId,
) -> bool {
    let mut visited = FxHashSet::default();
    apparent_type_has_signatures_rec(db, type_id, &mut visited)
}

fn apparent_type_has_signatures_rec(
    db: &dyn QueryDatabase,
    type_id: TypeId,
    visited: &mut FxHashSet<TypeId>,
) -> bool {
    if type_id.is_intrinsic() || !visited.insert(type_id) {
        return false;
    }
    match db.lookup(type_id) {
        Some(TypeData::Callable(shape_id)) => {
            let shape = db.callable_shape(shape_id);
            !shape.call_signatures.is_empty() || !shape.construct_signatures.is_empty()
        }
        Some(TypeData::Function(_)) => true,
        Some(TypeData::ReadonlyType(inner)) => apparent_type_has_signatures_rec(db, inner, visited),
        Some(TypeData::Intersection(list_id)) => db
            .type_list(list_id)
            .iter()
            .any(|&m| apparent_type_has_signatures_rec(db, m, visited)),
        Some(TypeData::Union(list_id)) => {
            let members = db.type_list(list_id);
            !members.is_empty()
                && members
                    .iter()
                    .all(|&m| apparent_type_has_signatures_rec(db, m, visited))
        }
        Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => info
            .constraint
            .is_some_and(|c| apparent_type_has_signatures_rec(db, c, visited)),
        Some(TypeData::Lazy(def_id)) => {
            db.resolve_lazy(def_id, db.as_type_database())
                .is_some_and(|resolved| {
                    resolved != type_id && apparent_type_has_signatures_rec(db, resolved, visited)
                })
        }
        // `typeof Class` (TypeQuery), generic class constructors (Application),
        // and other deferred forms only expose their constructor/call shape
        // after (resolver-backed) evaluation. Resolve once; the visited guard
        // prevents cycles.
        Some(_) => {
            let evaluated = db.evaluate_type(type_id);
            evaluated != type_id && apparent_type_has_signatures_rec(db, evaluated, visited)
        }
        None => false,
    }
}

/// Union of the construct signature return types applicable with zero explicit
/// type arguments (`tsc`'s `getConstructorsForTypeArguments`): only fully
/// defaulted signatures contribute, so still-generic overloads never leak free
/// type parameters into the instance type (`Map<any, any> | Map<K, V>` for
/// `class DraftMap extends Map`, #15248). Generic-only bases fall back to every
/// signature; `None` when there are no construct signatures. See
/// `construct_return_union_tests`.
pub fn get_construct_return_type_union(
    db: &dyn TypeDatabase,
    shape_id: crate::types::CallableShapeId,
) -> Option<TypeId> {
    let shape = db.callable_shape(shape_id);
    let defaulted = |sig: &CallSignature| sig.type_params.iter().all(|p| p.default.is_some());
    let any_defaulted = shape.construct_signatures.iter().any(defaulted);
    let returns: Vec<TypeId> = shape
        .construct_signatures
        .iter()
        .filter(|&sig| !any_defaulted || defaulted(sig))
        .map(|sig| sig.return_type)
        .collect();
    (!returns.is_empty()).then(|| crate::utils::union_or_single(db, returns))
}

/// Construct return type for any type: the applicable construct-signature union
/// for a Callable (see `get_construct_return_type_union`), the return type for a
/// constructor Function, `None` otherwise.
pub fn construct_return_type_for_type(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    use crate::type_queries::extended_constructors::InstanceTypeKind;
    match crate::type_queries::classify_for_instance_type(db, type_id) {
        InstanceTypeKind::Callable(shape_id) => get_construct_return_type_union(db, shape_id),
        InstanceTypeKind::Function(shape_id) => {
            let shape = db.function_shape(shape_id);
            if shape.is_constructor {
                Some(shape.return_type)
            } else {
                None
            }
        }
        InstanceTypeKind::Intersection(members) => {
            let returns = members
                .into_iter()
                .filter_map(|member| construct_return_type_for_type(db, member))
                .collect::<Vec<_>>();
            (!returns.is_empty()).then(|| crate::utils::intersection_or_single(db, returns))
        }
        InstanceTypeKind::Union(members) => {
            let mut returns = Vec::with_capacity(members.len());
            for member in members {
                returns.push(construct_return_type_for_type(db, member)?);
            }
            (!returns.is_empty()).then(|| crate::utils::union_or_single(db, returns))
        }
        InstanceTypeKind::Readonly(inner) => construct_return_type_for_type(db, inner),
        _ => None,
    }
}

/// Get the function shape for a function type.
///
/// Returns None if the type is not a Function.
pub fn get_function_shape(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<crate::types::FunctionShape>> {
    if type_id.is_intrinsic() {
        return None;
    }
    match db.lookup(type_id) {
        Some(TypeData::Function(shape_id)) => Some(db.function_shape(shape_id)),
        _ => None,
    }
}

/// Returns `true` if `type_id` is callable and its first call signature was declared with
/// method-shorthand syntax (`is_method = true`).
pub fn callable_first_sig_is_method(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    if let Some(shape) = get_function_shape(db, type_id) {
        return shape.is_method;
    }
    if let Some(shape) = get_callable_shape(db, type_id)
        && let Some(sig) = shape.call_signatures.first()
    {
        return sig.is_method;
    }
    false
}

/// Return a function type with all `ERROR` parameter and return positions rewritten to `ANY`.
///
/// Returns the original `type_id` when:
/// - it is not a function type
/// - the function shape does not contain `ERROR` in parameter or return positions
pub fn rewrite_function_error_slots_to_any(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    let Some(shape) = get_function_shape(db, type_id) else {
        return type_id;
    };

    fn rewrite_error_to_any_in_display_type(
        db: &dyn TypeDatabase,
        type_id: TypeId,
        seen: &mut FxHashMap<TypeId, TypeId>,
    ) -> TypeId {
        if type_id == TypeId::ERROR {
            return TypeId::ANY;
        }
        if type_id.is_intrinsic() {
            return type_id;
        }
        if let Some(rewritten) = seen.get(&type_id) {
            return *rewritten;
        }
        seen.insert(type_id, type_id);

        let rewritten = match db.lookup(type_id) {
            Some(TypeData::Object(shape_id)) => {
                let shape = db.object_shape(shape_id);
                let mut changed = false;
                let properties = shape
                    .properties
                    .iter()
                    .map(|prop| {
                        let type_id = rewrite_error_to_any_in_display_type(db, prop.type_id, seen);
                        let write_type =
                            rewrite_error_to_any_in_display_type(db, prop.write_type, seen);
                        changed |= type_id != prop.type_id || write_type != prop.write_type;
                        crate::types::PropertyInfo {
                            type_id,
                            write_type,
                            ..prop.clone()
                        }
                    })
                    .collect();
                if changed {
                    db.object_with_flags_and_symbol(properties, shape.flags, shape.symbol)
                } else {
                    type_id
                }
            }
            Some(TypeData::ObjectWithIndex(shape_id)) => {
                let shape = db.object_shape(shape_id);
                let mut changed = false;
                let properties = shape
                    .properties
                    .iter()
                    .map(|prop| {
                        let type_id = rewrite_error_to_any_in_display_type(db, prop.type_id, seen);
                        let write_type =
                            rewrite_error_to_any_in_display_type(db, prop.write_type, seen);
                        changed |= type_id != prop.type_id || write_type != prop.write_type;
                        crate::types::PropertyInfo {
                            type_id,
                            write_type,
                            ..prop.clone()
                        }
                    })
                    .collect();
                let string_index = shape.string_index.map(|mut index| {
                    let value_type =
                        rewrite_error_to_any_in_display_type(db, index.value_type, seen);
                    changed |= value_type != index.value_type;
                    index.value_type = value_type;
                    index
                });
                let number_index = shape.number_index.map(|mut index| {
                    let value_type =
                        rewrite_error_to_any_in_display_type(db, index.value_type, seen);
                    changed |= value_type != index.value_type;
                    index.value_type = value_type;
                    index
                });
                let symbol_index = shape.symbol_index.map(|mut index| {
                    let value_type =
                        rewrite_error_to_any_in_display_type(db, index.value_type, seen);
                    changed |= value_type != index.value_type;
                    index.value_type = value_type;
                    index
                });
                if changed {
                    db.object_with_index(crate::types::ObjectShape {
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
            Some(TypeData::Union(list_id)) => {
                let members = db.type_list(list_id);
                let mut changed = false;
                let rewritten = members
                    .iter()
                    .copied()
                    .map(|member| {
                        let rewritten = rewrite_error_to_any_in_display_type(db, member, seen);
                        changed |= rewritten != member;
                        rewritten
                    })
                    .collect();
                if changed {
                    db.union(rewritten)
                } else {
                    type_id
                }
            }
            Some(TypeData::Array(element)) => {
                let rewritten = rewrite_error_to_any_in_display_type(db, element, seen);
                if rewritten != element {
                    db.array(rewritten)
                } else {
                    type_id
                }
            }
            Some(TypeData::Tuple(list_id)) => {
                let elements = db.tuple_list(list_id);
                let mut changed = false;
                let rewritten = elements
                    .iter()
                    .map(|element| {
                        let type_id =
                            rewrite_error_to_any_in_display_type(db, element.type_id, seen);
                        changed |= type_id != element.type_id;
                        crate::types::TupleElement {
                            type_id,
                            ..*element
                        }
                    })
                    .collect();
                if changed {
                    db.tuple(rewritten)
                } else {
                    type_id
                }
            }
            _ => type_id,
        };
        seen.insert(type_id, rewritten);
        rewritten
    }

    let mut rewritten_types = FxHashMap::default();
    let params = shape
        .params
        .iter()
        .map(|p| crate::types::ParamInfo {
            type_id: rewrite_error_to_any_in_display_type(db, p.type_id, &mut rewritten_types),
            ..*p
        })
        .collect::<Vec<_>>();
    let return_type =
        rewrite_error_to_any_in_display_type(db, shape.return_type, &mut rewritten_types);
    let has_error = params
        .iter()
        .zip(shape.params.iter())
        .any(|(rewritten, original)| rewritten.type_id != original.type_id)
        || return_type != shape.return_type;
    if !has_error {
        return type_id;
    }

    db.function(crate::types::FunctionShape {
        type_params: shape.type_params.clone(),
        params,
        this_type: shape.this_type,
        return_type,
        type_predicate: shape.type_predicate,
        is_constructor: shape.is_constructor,
        is_method: shape.is_method,
    })
}

/// Return a copy of a function type with the `type_predicate` field cleared.
/// Returns `type_id` unchanged when it is not a function type or already has no predicate.
pub fn strip_function_type_predicate(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    let Some(shape) = get_function_shape(db, type_id) else {
        return type_id;
    };
    if shape.type_predicate.is_none() {
        return type_id;
    }
    db.function(crate::types::FunctionShape {
        type_params: shape.type_params.clone(),
        params: shape.params.clone(),
        this_type: shape.this_type,
        return_type: shape.return_type,
        type_predicate: None,
        is_constructor: shape.is_constructor,
        is_method: shape.is_method,
    })
}

/// Return a function type with the same signature but a replaced return type.
///
/// Returns the original `type_id` when:
/// - it is not a function type
/// - the existing return type already equals `new_return`
pub fn replace_function_return_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
    new_return: TypeId,
) -> TypeId {
    let Some(shape) = get_function_shape(db, type_id) else {
        return type_id;
    };
    if shape.return_type == new_return {
        return type_id;
    }

    db.function(crate::types::FunctionShape {
        type_params: shape.type_params.clone(),
        params: shape.params.clone(),
        this_type: shape.this_type,
        return_type: new_return,
        type_predicate: shape.type_predicate,
        is_constructor: shape.is_constructor,
        is_method: shape.is_method,
    })
}

/// Erase a generic function's type parameters by replacing them with `any`.
///
/// This mirrors TSC's `getErasedSignature` used in `isImplementationCompatibleWithOverload`.
/// Returns the original type when it is not a function or has no type parameters.
pub fn erase_function_type_params_to_any(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    let Some(shape) = get_function_shape(db, type_id) else {
        return type_id;
    };
    if shape.type_params.is_empty() {
        return type_id;
    }

    use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};

    let mut subst = TypeSubstitution::new();
    for tp in &shape.type_params {
        subst.insert(tp.name, TypeId::ANY);
    }

    let params = shape
        .params
        .iter()
        .map(|p| crate::types::ParamInfo {
            type_id: instantiate_type(db, p.type_id, &subst),
            ..*p
        })
        .collect();
    let return_type = instantiate_type(db, shape.return_type, &subst);
    let this_type = shape.this_type.map(|t| instantiate_type(db, t, &subst));

    db.function(crate::types::FunctionShape {
        type_params: Vec::new(), // erased
        params,
        this_type,
        return_type,
        type_predicate: shape.type_predicate,
        is_constructor: shape.is_constructor,
        is_method: shape.is_method,
    })
}

/// Get the conditional type info for a conditional type.
///
/// Returns None if the type is not a Conditional.
pub fn get_conditional_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<crate::types::ConditionalType>> {
    if type_id.is_intrinsic() {
        return None;
    }
    match db.lookup(type_id) {
        Some(TypeData::Conditional(cond_id)) => Some(db.conditional_type(cond_id)),
        _ => None,
    }
}

/// Classify a type body for argument preservation during application evaluation.
///
/// When instantiating `type Foo<T> = T extends Bar<infer U> ? U : never` with
/// `Foo<App<number>>`, the checker must decide whether to eagerly evaluate the
/// type argument `App<number>` to its structural form. If the body is a conditional
/// with `infer` patterns, evaluating Application-form args would destroy the
/// structure needed by `try_application_infer_match`.
///
/// Returns a classification that the checker uses to decide arg preservation policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyArgPreservation {
    /// No special preservation needed — evaluate args normally.
    EvaluateAll,
    /// Body is a conditional with `infer` in extends — preserve type-parameter
    /// and Application-form args so the solver's infer matching works correctly.
    ConditionalInfer,
    /// Body is a conditional with an Application containing `infer` in extends —
    /// preserve Application-form args specifically for Application-level infer matching.
    ConditionalApplicationInfer,
}

pub fn classify_body_for_arg_preservation(
    db: &dyn TypeDatabase,
    body_type: TypeId,
) -> BodyArgPreservation {
    let Some(cond) = get_conditional_type(db, body_type) else {
        return BodyArgPreservation::EvaluateAll;
    };
    if contains_infer_types_db(db, cond.extends_type) {
        // Check if extends type is an Application with infer (more specific case)
        if matches!(db.lookup(cond.extends_type), Some(TypeData::Application(_))) {
            return BodyArgPreservation::ConditionalApplicationInfer;
        }
        return BodyArgPreservation::ConditionalInfer;
    }
    BodyArgPreservation::EvaluateAll
}

/// Returns `true` if the generic body type contains structural type operations
/// that require type arguments to be in their concrete (expanded, non-Application)
/// form for correct evaluation.
///
/// When this returns `false`, Application-form type arguments can be safely
/// preserved during generic instantiation. Preserving the Application form
/// maintains generic identity so the solver's variance fast path can fire
/// during compatibility checks (e.g., `Map<any,any> <: Map<string,unknown>`
/// checks the type args via variance rather than expanding both to structural
/// objects and doing a deep property comparison).
///
/// Operations requiring concrete args:
/// - `Conditional`: `T extends Map<K,V> ? ... : ...` (needs T's structure)
/// - `IndexAccess`: `T[K]` (needs T's property shape)
/// - `KeyOf`: `keyof T` (needs T's property names)
/// - `Mapped`: `{ [P in keyof T]: ... }` (needs T's key space)
/// - `TemplateLiteral`: `` `${T}` `` (needs T to be string-like)
pub fn body_arg_requires_concrete_form(db: &dyn TypeDatabase, body_type: TypeId) -> bool {
    crate::visitors::visitor_predicates::contains_type_matching(db, body_type, |key| {
        matches!(
            key,
            TypeData::Conditional(_)
                | TypeData::IndexAccess(_, _)
                | TypeData::KeyOf(_)
                | TypeData::Mapped(_)
                | TypeData::TemplateLiteral(_)
        )
    })
}

/// Get the mapped type info for a mapped type.
///
/// Returns None if the type is not a Mapped type.
pub fn get_mapped_type(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<crate::types::MappedType>> {
    if type_id.is_intrinsic() {
        return None;
    }
    match db.lookup(type_id) {
        Some(TypeData::Mapped(mapped_id)) => Some(db.mapped_type(mapped_id)),
        _ => None,
    }
}

/// Get the mapped type id together with the mapped type info.
///
/// Returns None if the type is not a Mapped type.
pub fn get_mapped_type_with_id(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<(
    crate::types::MappedTypeId,
    std::sync::Arc<crate::types::MappedType>,
)> {
    if type_id.is_intrinsic() {
        return None;
    }
    match db.lookup(type_id) {
        Some(TypeData::Mapped(mapped_id)) => Some((mapped_id, db.mapped_type(mapped_id))),
        _ => None,
    }
}

/// Get the default type for a type-parameter-like type.
///
/// Returns None if the type is not a `TypeParameter` or `Infer`, or if it has no default.
pub fn get_type_parameter_default(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    // Fast path: intrinsics aren't `TypeParameter(_)` / `Infer(_)`.
    if type_id.is_intrinsic() {
        return None;
    }
    match db.lookup(type_id) {
        Some(TypeData::TypeParameter(info) | TypeData::Infer(info)) => info.default,
        _ => None,
    }
}

/// Get the type application info for a generic application type.
///
/// Returns None if the type is not an Application.
pub fn get_type_application(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> Option<std::sync::Arc<crate::types::TypeApplication>> {
    if type_id.is_intrinsic() {
        return None;
    }
    match db.lookup(type_id) {
        Some(TypeData::Application(app_id)) => Some(db.type_application(app_id)),
        _ => None,
    }
}

/// Get the index access components (object type and index type).
///
/// Returns None if the type is not an `IndexAccess`.
pub fn get_index_access_types(db: &dyn TypeDatabase, type_id: TypeId) -> Option<(TypeId, TypeId)> {
    // Fast path: intrinsics aren't `IndexAccess(_, _)`.
    if type_id.is_intrinsic() {
        return None;
    }
    match db.lookup(type_id) {
        Some(TypeData::IndexAccess(obj, idx)) => Some((obj, idx)),
        _ => None,
    }
}

pub fn contains_index_access_type(db: &dyn TypeDatabase, type_id: TypeId) -> bool {
    crate::contains_type_matching(db, type_id, |key| {
        matches!(key, TypeData::IndexAccess(_, _))
    })
}

pub fn index_access_type_arg_alias_hint(
    db: &dyn TypeDatabase,
    def_store: &crate::def::DefinitionStore,
    type_id: TypeId,
) -> Option<TypeId> {
    match db.lookup(type_id)? {
        TypeData::IndexAccess(object_type, _) => {
            index_access_object_type_arg_alias_hint(db, def_store, object_type)
        }
        TypeData::Intersection(list_id) | TypeData::Union(list_id) => db
            .type_list(list_id)
            .iter()
            .find_map(|&member| index_access_type_arg_alias_hint(db, def_store, member)),
        _ => None,
    }
}

fn index_access_object_type_arg_alias_hint(
    db: &dyn TypeDatabase,
    def_store: &crate::def::DefinitionStore,
    object_type: TypeId,
) -> Option<TypeId> {
    let app = get_type_application(db, object_type).or_else(|| {
        db.get_display_alias(object_type)
            .and_then(|alias| get_type_application(db, alias))
    })?;
    let &arg = app.args.first()?;
    let def_id = if let TypeData::Lazy(def_id) = db.lookup(arg)? {
        def_id
    } else {
        def_store.find_type_alias_by_body(arg).or_else(|| {
            let canonical_arg = canonical_alias_lookup_body(db, arg)?;
            def_store.find_type_alias_by_body(canonical_arg)
        })?
    };
    let def = def_store.get(def_id)?;
    (def.kind == crate::def::DefKind::TypeAlias && def.type_params.is_empty())
        .then(|| db.lazy(def_id))
}

fn canonical_alias_lookup_body(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    match db.lookup(type_id)? {
        TypeData::Union(list_id) => {
            let members = db.type_list(list_id);
            let canonical = db.union_literal_reduce(
                members
                    .iter()
                    .map(|&member| db.get_display_alias(member).unwrap_or(member))
                    .collect(),
            );
            (canonical != type_id).then_some(canonical)
        }
        TypeData::Intersection(list_id) => {
            let members = db.type_list(list_id);
            let canonical = db.intersection(
                members
                    .iter()
                    .map(|&member| db.get_display_alias(member).unwrap_or(member))
                    .collect(),
            );
            (canonical != type_id).then_some(canonical)
        }
        _ => None,
    }
}

/// Get the operand of a `KeyOf` type. Returns `Some(inner)` for `keyof T`.
pub fn get_keyof_operand(db: &dyn TypeDatabase, type_id: TypeId) -> Option<TypeId> {
    // Fast path: intrinsics aren't `KeyOf(_)`.
    if type_id.is_intrinsic() {
        return None;
    }
    match db.lookup(type_id) {
        Some(TypeData::KeyOf(inner)) => Some(inner),
        _ => None,
    }
}

/// Instantiate a mapped type template for a specific property key.
///
/// Instantiate a mapped type template for a specific property key, handling
/// name collisions between the mapped key parameter and outer type parameters.
///
/// When a mapped type template is `IndexAccess(T, K)` and the object type `T`
/// is a `TypeParameter` with the **same name atom** as the mapped key parameter,
/// name-based `TypeSubstitution` would incorrectly replace both `T` and `K`
/// with the key literal.  This happens with e.g. `Readonly<P>` where the lib
/// defines `type Readonly<T> = { readonly [P in keyof T]: T[P] }` and the user
/// has a type parameter also named `P`.
///
/// Returns `IndexAccess(T, key_literal)` when a collision is detected (bypassing
/// substitution), or the normally-substituted template otherwise.
pub fn instantiate_mapped_template_for_property(
    db: &dyn TypeDatabase,
    template: TypeId,
    key_param_name: Atom,
    key_literal: TypeId,
) -> TypeId {
    use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};

    // Check if template is IndexAccess(obj, key) where:
    // Case 1: The key is a TypeParameter matching the mapped key param.
    //   Construct Source[key_literal] directly to avoid name-based substitution
    //   corrupting the source when it contains a same-named outer type parameter
    //   (e.g., `Readonly<Props<P> & P>` where mapped key is also "P").
    // Case 2 (original): The object is a TypeParameter with the same name as the
    //   mapped key parameter (e.g., `Readonly<P>` where T=P from outer scope).
    if let Some((idx_obj, idx_key)) = get_index_access_types(db, template)
        && idx_obj != idx_key
    {
        if let Some(info) = get_type_parameter_info(db, idx_key)
            && info.name == key_param_name
        {
            return db.index_access(idx_obj, key_literal);
        }
        if let Some(info) = get_type_parameter_info(db, idx_obj)
            && info.name == key_param_name
        {
            return db.index_access(idx_obj, key_literal);
        }
    }

    // Normal path: substitute the key parameter name with the key literal
    let subst = TypeSubstitution::single(key_param_name, key_literal);
    instantiate_type(db, template, &subst)
}

fn intersection_has_impossible_literal_discriminants(
    db: &dyn TypeDatabase,
    type_id: TypeId,
) -> bool {
    // Concrete object intersections are eagerly merged for O(1) member lookup.
    // Inspect their retained structural origin: a merged required-`never`
    // discriminant is proof of a conflicting intersection, while the same
    // property on an authored object is not proof that the object is absent.
    let type_id = db
        .get_merged_intersection_origin(type_id)
        .unwrap_or(type_id);
    let Some(TypeData::Intersection(list_id)) = db.lookup(type_id) else {
        return false;
    };

    let mut discriminants: FxHashMap<Atom, Vec<TypeId>> = FxHashMap::default();
    for &member in db.type_list(list_id).iter() {
        let evaluated_member = crate::evaluation::evaluate::evaluate_type(db, member);
        let member = if evaluated_member != member {
            evaluated_member
        } else {
            member
        };
        let Some(shape) = get_object_shape(db, member) else {
            continue;
        };

        for prop in &shape.properties {
            let evaluated_prop = crate::evaluation::evaluate::evaluate_type(db, prop.type_id);
            let prop_type = if evaluated_prop != prop.type_id {
                evaluated_prop
            } else {
                prop.type_id
            };
            if !crate::type_queries::is_unit_type(db, prop_type) {
                continue;
            }

            let seen = discriminants.entry(prop.name).or_default();
            if seen.iter().any(|&other| {
                !crate::relations::subtype::is_subtype_of(db, prop_type, other)
                    && !crate::relations::subtype::is_subtype_of(db, other, prop_type)
            }) {
                return true;
            }
            if !seen.contains(&prop_type) {
                seen.push(prop_type);
            }
        }
    }

    false
}

/// Prune union members whose object/intersection shape is structurally
/// impossible because of conflicting literal discriminants, returning the
/// narrowed union (or `never` / the single survivor / the input unchanged).
/// A required `never` property does not make an object member impossible in
/// TypeScript and is therefore deliberately retained.
///
/// Pure function of the input union `TypeId`: it consults only structural
/// predicates over the immutable interned type `DAG` via resolver-free
/// `evaluate_type` / `is_subtype_of` walks, threading no resolver, substitution
/// environment, or compiler option. The result is therefore memoized
/// project-wide on the interner ([`TypePruneUnionCache`]) so the repeated
/// per-property-read prune walks that object-union property access issues
/// against the same discriminated-union `TypeId` collapse to O(1), mirroring the
/// sibling `contains_type_by_id` / `widen_type` pure-function memos.
pub fn prune_impossible_object_union_members(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    let Some(TypeData::Union(list_id)) = db.lookup(type_id) else {
        return type_id;
    };

    if let Some(cached) = db.prune_union_members_memo(type_id) {
        return cached;
    }

    let members = db.type_list(list_id);
    let retained: Vec<_> = members
        .iter()
        .copied()
        .filter(|&member| !intersection_has_impossible_literal_discriminants(db, member))
        .collect();

    let result = match retained.len() {
        0 => TypeId::NEVER,
        len if len == members.len() => type_id,
        1 => retained[0],
        _ => db.union_preserve_members(retained),
    };

    db.set_prune_union_members_memo(type_id, result);
    result
}
