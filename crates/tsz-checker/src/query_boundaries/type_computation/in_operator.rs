//! Reusable `in`-operator right-operand classification rules.
//!
//! These are the `WHAT` half of the `in`-operator checks: pure set-theoretic
//! rules over `TypeId` shapes that decide which diagnostic family a right
//! operand belongs to (TS2361 valid-RHS, TS2638 may-represent-primitive, and
//! the TS2322-vs-TS2638 routing for type-parameter-shaped operands). The
//! checker keeps the `WHERE` half — AST iteration, diagnostic anchoring, and
//! apparent-type display.
//!
//! Two leaf capabilities cannot be answered from the type database alone:
//! resolver-backed single-step evaluation (alias/application expansion) and the
//! dedicated TS2638 primitive-constraint relation outcome. The checker supplies
//! both through [`InOperatorRhsClassifier`], so the relation still routes
//! through the canonical `in_operator_primitive_constraint` request.

use crate::query_boundaries::state::checking;
use crate::query_boundaries::{common, dispatch as query};
use tsz_solver::TypeId;
use tsz_solver::construction::{QueryDatabase, TypeDatabase};

/// Depth cap for the `in`-operator RHS classifiers below. Each one walks a type
/// by evaluating it a single step at a time (`cx.evaluate`), which resolves
/// `Lazy(DefId)` references. A self-referential/pathological type can produce a
/// non-terminating chain of *distinct* interned `TypeId`s — every evaluation
/// step differs from the last — so the natural `evaluated != ty` termination
/// never fires and the recursion overflows the worker stack (SIGABRT on the
/// config-broken canary apps; `tsc` handles the same input without crashing).
///
/// A `TypeId` visited-set is unsound here: interned `TypeId`s are legitimately
/// shared across independent union/intersection branches, so a shared set would
/// short-circuit correct TS2361/TS2638/TS2322 results. Bounding the walk depth
/// is the right idiom — the same one the sibling type classifier
/// `for_of_element_type` uses at the same scale. Genuine type nesting
/// terminates far below this bound; only degenerate cycles reach it, and each
/// classifier returns its conservative answer (the one that manufactures no new
/// diagnostic) when it does.
const MAX_IN_OPERATOR_RHS_DEPTH: usize = 100;

/// Checker-supplied capabilities the `in`-operator RHS classifier needs beyond
/// pure type-database queries.
pub(crate) trait InOperatorRhsClassifier {
    /// The interned type database used for structural shape queries.
    fn types(&self) -> &dyn QueryDatabase;

    /// Resolver-backed single-step evaluation. Returns the input unchanged when
    /// the type cannot be reduced further.
    fn evaluate(&mut self, type_id: TypeId) -> TypeId;

    /// Whether `source` relates to `target` through the dedicated `in`-operator
    /// primitive-constraint relation request (TS2638 routing). Callers pass
    /// `string` as the probing source and a type-parameter constraint as the
    /// target.
    fn primitive_constraint_relates(&mut self, source: TypeId, target: TypeId) -> bool;
}

/// Whether `ty` is a valid right operand for `in` (assignable to `object`).
///
/// `any`/`error`/`object` and object-like types are valid; type parameters
/// defer to their constraint (unconstrained ones are invalid); unions require
/// every member valid; intersections require any member valid; otherwise a
/// single evaluation step is attempted before rejecting.
pub(crate) fn is_valid_in_operator_rhs(cx: &mut dyn InOperatorRhsClassifier, ty: TypeId) -> bool {
    is_valid_in_operator_rhs_with_depth(cx, ty, 0)
}

fn is_valid_in_operator_rhs_with_depth(
    cx: &mut dyn InOperatorRhsClassifier,
    ty: TypeId,
    depth: usize,
) -> bool {
    // See `MAX_IN_OPERATOR_RHS_DEPTH`. Conservatively treat a capped RHS as
    // valid so a degenerate `Lazy(DefId)` cycle manufactures no new TS2322
    // assignability failure.
    if depth > MAX_IN_OPERATOR_RHS_DEPTH {
        return true;
    }

    if matches!(ty, TypeId::ANY | TypeId::ERROR | TypeId::OBJECT) {
        return true;
    }

    // For type parameters, check if their constraint is assignable to object.
    // Unconstrained type params are NOT valid (could be primitive) → TS2322.
    if common::is_type_parameter_like(cx.types(), ty) {
        return match checking::type_parameter_constraint(cx.types(), ty) {
            Some(c) => is_valid_in_operator_rhs_with_depth(cx, c, depth + 1),
            None => false,
        };
    }

    if query::is_object_like_type(cx.types(), ty) {
        return true;
    }

    if let Some(members) = query::union_members(cx.types(), ty) {
        return members
            .iter()
            .all(|&member| is_valid_in_operator_rhs_with_depth(cx, member, depth + 1));
    }

    if let Some(members) = query::intersection_members(cx.types(), ty) {
        return members
            .iter()
            .any(|&member| is_valid_in_operator_rhs_with_depth(cx, member, depth + 1));
    }

    let evaluated = cx.evaluate(ty);
    if evaluated != ty {
        return is_valid_in_operator_rhs_with_depth(cx, evaluated, depth + 1);
    }

    false
}

/// Whether `ty` "may represent a primitive value" for TS2638.
///
/// In tsc, this fires for "instantiable" types (type parameters, conditional
/// types) whose constraint is missing or could accept primitive values.
/// Concrete object types like `{}` do NOT trigger TS2638 on their own — only
/// when they appear as the constraint of a type parameter that could be
/// instantiated with a primitive.
pub(crate) fn type_may_represent_primitive(
    cx: &mut dyn InOperatorRhsClassifier,
    ty: TypeId,
) -> bool {
    type_may_represent_primitive_with_depth(cx, ty, 0)
}

fn type_may_represent_primitive_with_depth(
    cx: &mut dyn InOperatorRhsClassifier,
    ty: TypeId,
    depth: usize,
) -> bool {
    // See `MAX_IN_OPERATOR_RHS_DEPTH`. Conservatively treat a capped type as
    // NOT primitive-representing — the same answer as the concrete-object
    // fall-through below — so a degenerate `Lazy(DefId)` cycle manufactures no
    // new TS2638.
    if depth > MAX_IN_OPERATOR_RHS_DEPTH {
        return false;
    }

    // The intrinsic `object` type excludes primitives by definition.
    if ty == TypeId::OBJECT {
        return false;
    }

    // `unknown` can represent any value including primitives — TS2638.
    if ty == TypeId::UNKNOWN {
        return true;
    }

    // Type parameters: check if constraint is missing or could be primitive.
    // A type param with no constraint or constraint `{}` may represent a
    // primitive because it could be instantiated with string, number, etc.
    if common::is_type_parameter_like(cx.types(), ty) {
        return match checking::type_parameter_constraint(cx.types(), ty) {
            None => true,                            // Unconstrained type param may be primitive
            Some(c) if c == TypeId::OBJECT => false, // `extends object` excludes primitives
            Some(c) => {
                // Check if the constraint itself could accept primitives.
                // This handles `T extends {}` (may represent primitive) vs
                // `T extends object` (may not) vs `T extends { a: number }`
                // (may not).
                if type_may_represent_primitive_with_depth(cx, c, depth + 1) {
                    return true;
                }
                // For concrete constraints, check if a primitive is assignable.
                cx.primitive_constraint_relates(TypeId::STRING, c)
            }
        };
    }

    // Union: any member may represent primitive.
    if let Some(members) = common::union_members(cx.types(), ty) {
        return members
            .iter()
            .any(|&m| type_may_represent_primitive_with_depth(cx, m, depth + 1));
    }

    // Intersection: `T & {}` still may represent a primitive because `{}`
    // only removes nullish values. However, `T & object`, `T & { x: ... }`,
    // and `T & Interface` exclude primitives through the object-like member.
    if let Some(members) = common::intersection_members(cx.types(), ty) {
        let has_instantiable_primitive_member = members.iter().any(|&member| {
            common::is_type_parameter_like(cx.types(), member)
                && type_may_represent_primitive_with_depth(cx, member, depth + 1)
        });
        if has_instantiable_primitive_member
            && !members.iter().any(|&member| {
                in_operator_intersection_member_excludes_primitive(cx.types(), member)
            })
        {
            return true;
        }

        return members
            .iter()
            .all(|&m| type_may_represent_primitive_with_depth(cx, m, depth + 1));
    }

    let evaluated = cx.evaluate(ty);
    if evaluated != ty {
        return type_may_represent_primitive_with_depth(cx, evaluated, depth + 1);
    }

    // Concrete object types are NOT considered "may represent primitive" —
    // only type parameters can be instantiated with primitives at runtime.
    false
}

/// True when `ty` is an `in`-operator RHS shape that tsc reports via TS2322
/// (assignability to `object`) rather than TS2638 (primitive runtime warning).
///
/// tsc routes these to the assignability gateway:
/// - bare type parameters (`T`)
/// - unions that contain a type parameter/primitive assignability member
///   (`T | U`, `string | number | T`, `T | { a: string }`)
/// - intersections whose every member is a type parameter or primitive
///   constraint (`T & U`, `T & (0 | 1 | 2)`)
///
/// It keeps TS2638 for shapes whose apparent type is reported with a
/// `NonNullable<T>`-style message — typically intersections with `{}`-shaped
/// object constraints. For those, the empty-object member excludes some nullish
/// cases without committing to the `object` constraint, and tsc emits TS2638
/// with the `NonNullable<T>` apparent-type display.
pub(crate) fn in_rhs_is_type_parameter_assignability_shape(
    db: &dyn TypeDatabase,
    ty: TypeId,
) -> bool {
    if common::is_type_parameter_like(db, ty) {
        return true;
    }
    if let Some(members) = common::union_members(db, ty) {
        // A union containing a bare generic or primitive constituent is
        // reported through assignability to `object`, even when other
        // constituents are object-like. This is the shape produced by a
        // false branch of `("a" in x && "b" in x)`: `T | (T & Record<...>)`.
        return members
            .iter()
            .any(|&m| in_rhs_is_type_parameter_assignability_shape(db, m));
    }
    if let Some(members) = common::intersection_members(db, ty) {
        // Intersections with an empty-object-constraint member (e.g. `T & {}`,
        // `T & EmptyAlias`) collapse to `NonNullable<T>` in tsc's apparent-type
        // rendering and stay on the TS2638 path. Recognize that by requiring
        // every member to be either a type parameter or a primitive — if any
        // member is an empty-object shape, defer to TS2638.
        if members.iter().any(|&m| {
            common::object_shape_for_type(db, m).is_some_and(|shape| shape.properties.is_empty())
        }) {
            return false;
        }
        return members
            .iter()
            .all(|&m| in_rhs_is_type_parameter_assignability_shape(db, m));
    }
    // Concrete primitives (string, number, ...) are also routed to the
    // assignability gateway when they're combined with type parameters in a
    // union / intersection that we already verified above. A bare primitive (no
    // generics involved) keeps the TS2638 path because the user can fix it
    // without touching a generic position.
    common::is_primitive_type(db, ty)
}

/// Whether `ty` contains an empty-object (`{}`) shape, recursing through unions
/// and a single evaluation step.
pub(crate) fn in_operator_type_contains_empty_object_shape(
    cx: &mut dyn InOperatorRhsClassifier,
    ty: TypeId,
) -> bool {
    in_operator_type_contains_empty_object_shape_with_depth(cx, ty, 0)
}

fn in_operator_type_contains_empty_object_shape_with_depth(
    cx: &mut dyn InOperatorRhsClassifier,
    ty: TypeId,
    depth: usize,
) -> bool {
    // See `MAX_IN_OPERATOR_RHS_DEPTH`. Conservatively report "no empty-object
    // shape" — the same answer as the no-further-evaluation fall-through — when
    // a degenerate `Lazy(DefId)` cycle exhausts the depth budget.
    if depth > MAX_IN_OPERATOR_RHS_DEPTH {
        return false;
    }

    if common::is_empty_object_type(cx.types(), ty) {
        return true;
    }

    if let Some(members) = common::union_members(cx.types(), ty) {
        return members.iter().any(|&member| {
            in_operator_type_contains_empty_object_shape_with_depth(cx, member, depth + 1)
        });
    }

    let evaluated = cx.evaluate(ty);
    evaluated != ty
        && in_operator_type_contains_empty_object_shape_with_depth(cx, evaluated, depth + 1)
}

/// Whether an intersection member excludes primitives (an object-like, non-empty
/// shape, `object`, or a type parameter/composite that reduces to one).
pub(crate) fn in_operator_intersection_member_excludes_primitive(
    db: &dyn TypeDatabase,
    ty: TypeId,
) -> bool {
    if ty == TypeId::OBJECT {
        return true;
    }

    if common::is_type_parameter_like(db, ty) {
        return checking::type_parameter_constraint(db, ty).is_some_and(|constraint| {
            in_operator_intersection_member_excludes_primitive(db, constraint)
        });
    }

    if let Some(members) = query::union_members(db, ty) {
        return members
            .iter()
            .all(|&member| in_operator_intersection_member_excludes_primitive(db, member));
    }

    if let Some(members) = query::intersection_members(db, ty) {
        return members
            .iter()
            .any(|&member| in_operator_intersection_member_excludes_primitive(db, member));
    }

    query::is_object_like_type(db, ty) && !common::is_empty_object_type(db, ty)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsz_solver::PropertyInfo;
    use tsz_solver::construction::TypeInterner;

    /// Test classifier with identity evaluation and a configurable
    /// primitive-constraint relation. The structural rules under test do not
    /// require resolver-backed evaluation, so identity evaluation is sufficient.
    struct TestClassifier<'a> {
        db: &'a TypeInterner,
        string_relates_to: Vec<TypeId>,
    }

    impl InOperatorRhsClassifier for TestClassifier<'_> {
        fn types(&self) -> &dyn QueryDatabase {
            self.db
        }
        fn evaluate(&mut self, type_id: TypeId) -> TypeId {
            type_id
        }
        fn primitive_constraint_relates(&mut self, source: TypeId, target: TypeId) -> bool {
            source == TypeId::STRING && self.string_relates_to.contains(&target)
        }
    }

    fn classifier(db: &TypeInterner) -> TestClassifier<'_> {
        TestClassifier {
            db,
            string_relates_to: Vec::new(),
        }
    }

    #[test]
    fn object_and_object_like_are_valid_rhs() {
        let db = TypeInterner::new();
        let mut cx = classifier(&db);
        assert!(is_valid_in_operator_rhs(&mut cx, TypeId::OBJECT));
        assert!(is_valid_in_operator_rhs(&mut cx, TypeId::ANY));
        let obj = db.object_fresh(vec![]);
        assert!(is_valid_in_operator_rhs(&mut cx, obj));
    }

    #[test]
    fn bare_primitive_is_not_valid_rhs() {
        let db = TypeInterner::new();
        let mut cx = classifier(&db);
        assert!(!is_valid_in_operator_rhs(&mut cx, TypeId::STRING));
        assert!(!is_valid_in_operator_rhs(&mut cx, TypeId::NUMBER));
    }

    #[test]
    fn union_requires_every_member_valid() {
        let db = TypeInterner::new();
        let mut cx = classifier(&db);
        let obj = db.object_fresh(vec![]);
        let all_valid = db.union(vec![obj, TypeId::OBJECT]);
        assert!(is_valid_in_operator_rhs(&mut cx, all_valid));
        let mixed = db.union(vec![obj, TypeId::STRING]);
        assert!(!is_valid_in_operator_rhs(&mut cx, mixed));
    }

    #[test]
    fn intersection_requires_any_member_valid() {
        let db = TypeInterner::new();
        let mut cx = classifier(&db);
        let obj = db.object_fresh(vec![]);
        let inter = db.intersection(vec![TypeId::STRING, obj]);
        assert!(is_valid_in_operator_rhs(&mut cx, inter));
    }

    #[test]
    fn unknown_may_represent_primitive() {
        let db = TypeInterner::new();
        let mut cx = classifier(&db);
        assert!(type_may_represent_primitive(&mut cx, TypeId::UNKNOWN));
        assert!(!type_may_represent_primitive(&mut cx, TypeId::OBJECT));
    }

    #[test]
    fn concrete_object_does_not_represent_primitive() {
        let db = TypeInterner::new();
        let mut cx = classifier(&db);
        let obj = db.object_fresh(vec![]);
        assert!(!type_may_represent_primitive(&mut cx, obj));
    }

    #[test]
    fn empty_object_shape_detection_walks_unions() {
        let db = TypeInterner::new();
        let mut cx = classifier(&db);
        let empty = db.object_fresh(vec![]);
        let with_empty = db.union(vec![TypeId::NUMBER, empty]);
        assert!(in_operator_type_contains_empty_object_shape(
            &mut cx, with_empty
        ));
        assert!(!in_operator_type_contains_empty_object_shape(
            &mut cx,
            TypeId::NUMBER
        ));
    }

    /// A classifier whose single-step evaluation never reaches a fixpoint: it
    /// ping-pongs between two leaf types, mimicking a self-referential
    /// `Lazy(DefId)` chain where `evaluate` yields an endless run of *distinct*
    /// interned `TypeId`s. Pre-cap, feeding either endpoint to the classifier
    /// walks recursed until the worker stack overflowed (SIGABRT on the
    /// `outline` canary app); `MAX_IN_OPERATOR_RHS_DEPTH` makes each walk
    /// terminate at its conservative answer instead.
    struct CyclingClassifier<'a> {
        db: &'a TypeInterner,
        a: TypeId,
        b: TypeId,
    }

    impl InOperatorRhsClassifier for CyclingClassifier<'_> {
        fn types(&self) -> &dyn QueryDatabase {
            self.db
        }
        fn evaluate(&mut self, type_id: TypeId) -> TypeId {
            // Neither endpoint is ever its own evaluation fixpoint, so
            // `evaluated != ty` holds forever without a depth cap.
            if type_id == self.a { self.b } else { self.a }
        }
        fn primitive_constraint_relates(&mut self, _source: TypeId, _target: TypeId) -> bool {
            false
        }
    }

    /// The three evaluate-driven classifiers must terminate on a non-terminating
    /// evaluation cycle and return the conservative answer (the one that
    /// manufactures no new diagnostic), independent of which leaf `TypeId`s form
    /// the cycle.
    fn assert_cycle_terminates_conservatively(a: TypeId, b: TypeId) {
        let db = TypeInterner::new();
        let mut cx = CyclingClassifier { db: &db, a, b };

        // Conservative for TS2638: not primitive-representing (no TS2638).
        assert!(!type_may_represent_primitive(&mut cx, a));
        // Conservative for TS2322: treat as a valid RHS (no assignability error).
        assert!(is_valid_in_operator_rhs(&mut cx, a));
        // Conservative for the empty-object probe: report absence.
        assert!(!in_operator_type_contains_empty_object_shape(&mut cx, a));
    }

    #[test]
    fn evaluate_cycle_string_number_terminates_conservatively() {
        assert_cycle_terminates_conservatively(TypeId::STRING, TypeId::NUMBER);
    }

    #[test]
    fn evaluate_cycle_boolean_bigint_terminates_conservatively() {
        // A different leaf pair proves the cap is a depth mechanism, not a
        // fast-path keyed on specific `TypeId`s.
        assert_cycle_terminates_conservatively(TypeId::BOOLEAN, TypeId::BIGINT);
    }

    #[test]
    fn bare_primitive_keeps_ts2638_path_not_assignability_shape() {
        let db = TypeInterner::new();
        // A bare primitive is an assignability-shape only inside a generic
        // union/intersection; on its own `is_primitive_type` returns true.
        assert!(in_rhs_is_type_parameter_assignability_shape(
            &db,
            TypeId::STRING
        ));
        let obj = db.object_fresh(vec![PropertyInfo::new(
            db.intern_string("a"),
            TypeId::NUMBER,
        )]);
        assert!(!in_rhs_is_type_parameter_assignability_shape(&db, obj));
    }
}
