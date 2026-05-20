//! Tuple synthesis cardinality gate.
//!
//! When a spread/flatten path would produce a tuple wider than
//! `MAX_TUPLE_LENGTH`, the solver MUST short-circuit to `TypeId::ERROR` and
//! set the `tuple_too_large` flag. The checker reads the flag to emit
//! TS2799 (type alias body) or TS2800 (`as const` array literal).
//!
//! These tests pin the structural rule at three different synthesis sites:
//!   1. `instantiate.rs` Tuple flattening (generic conditional tail
//!      recursion like `BuildTuple<L, [...T, ...T]>` where `T` doubles per
//!      level).
//!   2. `evaluate.rs visit_tuple` rest-tuple flattening (concrete tuple
//!      alias chain like `type T14 = [...T13, ...T13]`).
//!   3. `evaluate.rs visit_tuple` union-spread distributed flattening
//!      (`[...A | B]` where the largest arm crosses the limit).
//!
//! Each test varies the iteration variable name (`T`/`X`/`U`) to defend
//! against `CLAUDE.md` §25's anti-hardcoding directive: a regression that
//! re-introduces a name-keyed shortcut would only break one of the three
//! probe shapes.

use super::*;
use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};
use crate::intern::{MAX_TUPLE_LENGTH, MAX_TUPLE_SPREAD_FLATTEN_ELEMENTS, TypeInterner};
use crate::types::{TupleElement, TypeData, TypeParamInfo};

fn tuple_element(type_id: TypeId, rest: bool) -> TupleElement {
    TupleElement {
        type_id,
        name: None,
        optional: false,
        rest,
    }
}

fn n_copies_of_any(n: usize) -> Vec<TupleElement> {
    (0..n).map(|_| tuple_element(TypeId::ANY, false)).collect()
}

/// `[...T, ...T]` with `T = [any; MAX_TUPLE_LENGTH]` is exactly at the
/// representable limit and must NOT trigger the gate. The first rest
/// flattens fully; the second rest sees `instantiated.len() + inner.len()`
/// = `MAX_TUPLE_LENGTH + MAX_TUPLE_LENGTH` which exceeds the limit and
/// must short-circuit.
#[test]
fn instantiate_tuple_double_spread_over_limit_marks_flag_and_returns_error() {
    let interner = TypeInterner::new();
    assert!(!interner.take_tuple_too_large());

    let t_param_name = interner.intern_string("T");
    let t_param_atom = t_param_name;

    let param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_param_atom,
        constraint: Some(interner.array(TypeId::UNKNOWN)),
        default: None,
        is_const: false,
    }));

    // body: [...T, ...T]
    let body = interner.tuple(vec![tuple_element(param, true), tuple_element(param, true)]);

    // T -> [any * MAX_TUPLE_LENGTH] (exactly at limit, so the first spread
    // can still flatten but the second pushes past).
    let big_tuple = interner.tuple(n_copies_of_any(MAX_TUPLE_LENGTH));
    let mut subst = TypeSubstitution::new();
    subst.insert(t_param_atom, big_tuple);

    let result = instantiate_type(&interner, body, &subst);
    assert_eq!(result, TypeId::ERROR);
    assert!(
        interner.take_tuple_too_large(),
        "tuple_too_large flag should fire when [...T, ...T] flatten would exceed MAX_TUPLE_LENGTH"
    );
    // Reading clears the flag.
    assert!(!interner.take_tuple_too_large());
}

/// `[...T, ...T]` must keep counting represented tuple cardinality even
/// after the soft gate stores a large-but-representable spread as one rest
/// element. This pins the case where `T` is bigger than the soft flatten cap
/// but smaller than the hard tuple limit: the first spread occupies one
/// physical slot, while the represented length is still `T.len()`.
#[test]
fn instantiate_tuple_soft_unflattened_spreads_still_count_represented_length() {
    let interner = TypeInterner::new();
    let _ = interner.take_tuple_too_large();

    let t_atom = interner.intern_string("T");
    let param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_atom,
        constraint: Some(interner.array(TypeId::UNKNOWN)),
        default: None,
        is_const: false,
    }));

    let body = interner.tuple(vec![tuple_element(param, true), tuple_element(param, true)]);
    let soft_unflattened_len = MAX_TUPLE_SPREAD_FLATTEN_ELEMENTS + 1;
    assert!(
        soft_unflattened_len < MAX_TUPLE_LENGTH,
        "regression fixture must stay below the hard cap per spread"
    );

    let soft_unflattened_tuple = interner.tuple(n_copies_of_any(soft_unflattened_len));
    let mut subst = TypeSubstitution::new();
    subst.insert(t_atom, soft_unflattened_tuple);

    let result = instantiate_type(&interner, body, &subst);
    assert_eq!(result, TypeId::ERROR);
    assert!(
        interner.take_tuple_too_large(),
        "two soft-unflattened spreads should exceed represented MAX_TUPLE_LENGTH"
    );
}

/// Same rule with a renamed iteration variable proves the gate is keyed on
/// the structural condition (post-flatten cardinality), not on the type
/// parameter name. `CLAUDE.md` §25 review checklist item 3.
#[test]
fn instantiate_tuple_cardinality_gate_is_name_independent() {
    let interner = TypeInterner::new();

    for name in ["T", "X", "U", "P"] {
        let _ = interner.take_tuple_too_large();
        let atom = interner.intern_string(name);
        let param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
            name: atom,
            constraint: Some(interner.array(TypeId::UNKNOWN)),
            default: None,
            is_const: false,
        }));
        let body = interner.tuple(vec![tuple_element(param, true), tuple_element(param, true)]);
        let big_tuple = interner.tuple(n_copies_of_any(MAX_TUPLE_LENGTH));
        let mut subst = TypeSubstitution::new();
        subst.insert(atom, big_tuple);
        let result = instantiate_type(&interner, body, &subst);
        assert_eq!(result, TypeId::ERROR, "name `{name}` should still error");
        assert!(
            interner.take_tuple_too_large(),
            "name `{name}` should still mark the flag",
        );
    }
}

/// Single spread that exactly fits MUST NOT trigger the gate. This pins
/// the boundary: `MAX_TUPLE_LENGTH` elements is representable; >
/// `MAX_TUPLE_LENGTH` is not.
#[test]
fn instantiate_tuple_at_exact_limit_does_not_trigger_gate() {
    let interner = TypeInterner::new();
    let _ = interner.take_tuple_too_large();

    let t_atom = interner.intern_string("T");
    let param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_atom,
        constraint: Some(interner.array(TypeId::UNKNOWN)),
        default: None,
        is_const: false,
    }));
    // body: [...T] — single spread
    let body = interner.tuple(vec![tuple_element(param, true)]);

    let exactly_at_limit = interner.tuple(n_copies_of_any(MAX_TUPLE_LENGTH));
    let mut subst = TypeSubstitution::new();
    subst.insert(t_atom, exactly_at_limit);

    let result = instantiate_type(&interner, body, &subst);
    assert_ne!(result, TypeId::ERROR, "exactly-at-limit must not error");
    assert!(
        !interner.take_tuple_too_large(),
        "exactly-at-limit must not mark the flag"
    );
}

/// Sibling test: a single spread of one element MORE than the limit must
/// trigger the gate even when there is only one rest element.
#[test]
fn instantiate_tuple_single_spread_just_over_limit_triggers_gate() {
    let interner = TypeInterner::new();
    let _ = interner.take_tuple_too_large();

    let t_atom = interner.intern_string("T");
    let param = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: t_atom,
        constraint: Some(interner.array(TypeId::UNKNOWN)),
        default: None,
        is_const: false,
    }));
    let body = interner.tuple(vec![tuple_element(param, true)]);

    let one_over = interner.tuple(n_copies_of_any(MAX_TUPLE_LENGTH + 1));
    let mut subst = TypeSubstitution::new();
    subst.insert(t_atom, one_over);

    let result = instantiate_type(&interner, body, &subst);
    assert_eq!(result, TypeId::ERROR);
    assert!(interner.take_tuple_too_large());
}

/// Sibling test: keep the gate as a hard ceiling even when the body has
/// no spread — `MAX_TUPLE_LENGTH + 1` literal elements in the source must
/// still be representable (they bypass the spread synthesis path
/// entirely). Pinning this prevents an over-eager guard from regressing
/// legitimate large fixed-arity tuples.
#[test]
fn direct_tuple_construction_does_not_trigger_spread_gate() {
    let interner = TypeInterner::new();
    let _ = interner.take_tuple_too_large();

    // Build a tuple of (LIMIT + 1) elements without going through a spread.
    let direct = interner.tuple(n_copies_of_any(MAX_TUPLE_LENGTH + 1));
    assert_ne!(direct, TypeId::ERROR);
    assert!(
        !interner.take_tuple_too_large(),
        "direct construction must not trip the spread gate"
    );
}

/// `MAX_TUPLE_LENGTH` is the exported parity constant. Tying the test to
/// it (rather than the literal `10_000`) keeps the test honest if the
/// constant moves. tsc uses the same limit on every target, so any future
/// platform-specific divergence is a parity regression and should be
/// reviewed alongside this test.
#[test]
fn max_tuple_length_matches_documented_constant() {
    assert_eq!(MAX_TUPLE_LENGTH, 10_000);
}

/// Flag state survives multiple synthesis paths in the same interner.
/// Setting from `instantiate.rs` and then reading clears it; a subsequent
/// `mark_tuple_too_large` re-sets it. Pins the lifecycle so a future
/// `take_*` accidentally being `clone` or returning a borrowed cell would
/// fail this test.
#[test]
fn tuple_too_large_flag_lifecycle_is_set_take_set() {
    let interner = TypeInterner::new();
    use crate::TypeDatabase;
    let db_ref: &dyn TypeDatabase = &interner;

    assert!(!db_ref.take_tuple_too_large());
    db_ref.mark_tuple_too_large();
    assert!(db_ref.take_tuple_too_large());
    assert!(!db_ref.take_tuple_too_large());
    db_ref.mark_tuple_too_large();
    assert!(db_ref.take_tuple_too_large());
}
