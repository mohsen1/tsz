//! Conditional-type distribution helpers for callers that hold unevaluated
//! conditionals (e.g. `getErasedSignature`-style overload/implementation
//! return comparison).

use crate::construction::TypeDatabase;
use crate::evaluation::evaluate::evaluate_type;
use crate::types::{TypeData, TypeId};

/// Resolve a conditional type whose check type is `any` to the union of both
/// branches, matching tsc's `getConditionalType` (a conditional with an `any`
/// check type evaluates to `trueType | falseType` — the wildcard takes both
/// arms). Non-conditional types, and conditionals with a non-`any` check type,
/// are returned unchanged.
///
/// General conditional evaluation (`evaluate_type`) already performs this
/// union, so this helper exists for callers that hold a still-unevaluated
/// conditional and hand it straight to a relation without an evaluation step —
/// notably `getErasedSignature`-style overload/implementation return comparison,
/// where the overload's type parameters have been substituted with `any` and the
/// resulting `any extends U ? A : B` reaches the relation raw. Distributing here
/// keeps the false branch (which the relation would otherwise drop by resolving
/// the `any` check to a single true-branch pick).
///
/// The conditional itself can be one alias hop away (`type Foo<T> = T extends
/// U ? A : B` referenced as `Foo<any>`): `type_id` then carries an
/// `Application`/`Lazy` alias reference, not `TypeData::Conditional`, until
/// evaluated. Evaluate first so this still recognizes the shape through the
/// alias, matching how tsc resolves a conditional-type-alias reference before
/// comparing signature return types.
pub fn distribute_any_check_conditional(db: &dyn TypeDatabase, type_id: TypeId) -> TypeId {
    let evaluated = evaluate_type(db, type_id);
    let Some(TypeData::Conditional(cid)) = db.lookup(evaluated) else {
        return type_id;
    };
    let cond = db.conditional_type(cid);
    if cond.check_type != TypeId::ANY {
        return type_id;
    }
    let true_branch = evaluate_type(db, cond.true_type);
    let false_branch = evaluate_type(db, cond.false_type);
    db.union2(true_branch, false_branch)
}
