//! Tests for tuple spread cardinality gates in instantiation.
//!
//! The instantiator has two gates:
//!
//! - **Soft gate** (`MAX_TUPLE_SPREAD_FLATTEN_ELEMENTS = 8192`): When
//!   flattening a spread would add more than 8192 *physical* slots, keep the
//!   spread as a single rest element instead of inlining it.
//!
//! - **Hard gate** (`MAX_REPRESENTABLE_TUPLE_LENGTH = 10_000`): Even if the
//!   soft gate keeps large spreads as single physical slots, the *represented*
//!   cardinality (sum of each spread's inner element count) can still exceed
//!   the limit. When it does, the instantiator sets the `tuple_too_large` flag
//!   and returns `TypeId::ERROR`.
//!
//! These tests verify both gates and their interaction.

use super::*;
use crate::TypeInterner;
use crate::caches::db::TypeTupleLimitSignal;
use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};
use crate::types::{TupleElement, TypeData, TypeParamInfo};
use tsz_common::limits::MAX_REPRESENTABLE_TUPLE_LENGTH;

// ── helpers ──────────────────────────────────────────────────────────────────

/// Build an unconstrained type parameter with the given name.
fn make_param(interner: &TypeInterner, name: &str) -> TypeId {
    let atom = interner.intern_string(name);
    interner.intern(TypeData::TypeParameter(TypeParamInfo::simple(atom)))
}

/// Build a concrete tuple of `len` `TypeId::NUMBER` elements (no rest, no optional).
fn make_concrete_tuple(interner: &TypeInterner, len: usize) -> TypeId {
    let elements: Vec<TupleElement> = (0..len)
        .map(|_| TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        })
        .collect();
    interner.tuple(elements)
}

/// Build a tuple `[...T, ...T]` where `T` is a type parameter.
fn make_double_spread_tuple(interner: &TypeInterner, param: TypeId) -> TypeId {
    interner.tuple(vec![
        TupleElement {
            type_id: param,
            name: None,
            optional: false,
            rest: true,
        },
        TupleElement {
            type_id: param,
            name: None,
            optional: false,
            rest: true,
        },
    ])
}

// ── tests ─────────────────────────────────────────────────────────────────────

/// When `T` has `MAX_REPRESENTABLE_TUPLE_LENGTH / 2 + 1` elements and the body
/// is `[...T, ...T]`, the represented cardinality exceeds the limit.
/// The hard gate must fire, set the flag, and return `TypeId::ERROR`.
///
/// This is the canonical case the hard gate was introduced to catch: each
/// individual spread is small enough to pass the soft gate, but together they
/// exceed the representable limit.
#[test]
fn instantiate_tuple_double_spread_over_limit_marks_flag_and_returns_error() {
    let interner = TypeInterner::new();

    // T = concrete tuple of (MAX/2 + 1) elements → two spreads = MAX+2 > MAX
    let half_plus_one = MAX_REPRESENTABLE_TUPLE_LENGTH / 2 + 1;
    let concrete = make_concrete_tuple(&interner, half_plus_one);

    for param_name in ["T", "K"] {
        // Clear any residual flag from previous iteration.
        let _ = interner.take_tuple_too_large();

        let param = make_param(&interner, param_name);
        let body = make_double_spread_tuple(&interner, param);

        let atom = interner.intern_string(param_name);
        let subst = TypeSubstitution::single(atom, concrete);

        let result = instantiate_type(&interner, body, &subst);

        assert_eq!(
            result,
            TypeId::ERROR,
            "param_name={param_name}: expected ERROR when represented cardinality exceeds limit"
        );
        assert!(
            interner.take_tuple_too_large(),
            "param_name={param_name}: expected tuple_too_large flag to be set"
        );
    }
}

/// The CRITICAL missing case: `T` has `MAX_TUPLE_SPREAD_FLATTEN_ELEMENTS + 1`
/// (= 8193) elements. Each spread stays as one physical rest slot (soft gate),
/// but the represented cardinality is 8193 + 8193 = 16386 > `10_000`.
///
/// The soft gate alone would not catch this — only the hard gate does.
///
/// We use the literal value 8193 (one above the private soft cap of 8192) and
/// explain it via the comment rather than importing the private constant.
#[test]
fn instantiate_tuple_soft_unflattened_spreads_still_count_represented_length() {
    let interner = TypeInterner::new();

    // 8193 = MAX_TUPLE_SPREAD_FLATTEN_ELEMENTS + 1 (soft cap is 8192, private).
    // Two of these = 16386 represented elements, well above MAX_REPRESENTABLE_TUPLE_LENGTH.
    let over_soft_cap: usize = 8193;
    assert!(
        over_soft_cap * 2 > MAX_REPRESENTABLE_TUPLE_LENGTH,
        "test pre-condition: 2 × {over_soft_cap} must exceed MAX_REPRESENTABLE_TUPLE_LENGTH"
    );

    let concrete = make_concrete_tuple(&interner, over_soft_cap);

    for param_name in ["T", "X"] {
        let _ = interner.take_tuple_too_large();

        let param = make_param(&interner, param_name);
        let body = make_double_spread_tuple(&interner, param);

        let atom = interner.intern_string(param_name);
        let subst = TypeSubstitution::single(atom, concrete);

        let result = instantiate_type(&interner, body, &subst);

        assert_eq!(
            result,
            TypeId::ERROR,
            "param_name={param_name}: soft-unflattened spreads with represented total \
             {total} > {MAX_REPRESENTABLE_TUPLE_LENGTH} must trigger hard gate",
            total = over_soft_cap * 2
        );
        assert!(
            interner.take_tuple_too_large(),
            "param_name={param_name}: tuple_too_large flag must be set by hard gate"
        );
    }
}

/// Verify the gate is triggered regardless of the type-parameter name.
/// If the implementation were keyed on the name it would fail with uncommon names.
#[test]
fn instantiate_tuple_cardinality_gate_is_name_independent() {
    let interner = TypeInterner::new();

    let half_plus_one = MAX_REPRESENTABLE_TUPLE_LENGTH / 2 + 1;
    let concrete = make_concrete_tuple(&interner, half_plus_one);

    for param_name in ["T", "Item", "Element", "Spread", "MyVeryLongParameterName"] {
        let _ = interner.take_tuple_too_large();

        let param = make_param(&interner, param_name);
        let body = make_double_spread_tuple(&interner, param);

        let atom = interner.intern_string(param_name);
        let subst = TypeSubstitution::single(atom, concrete);

        let result = instantiate_type(&interner, body, &subst);

        assert_eq!(
            result,
            TypeId::ERROR,
            "param_name={param_name}: gate must be name-independent"
        );
        assert!(
            interner.take_tuple_too_large(),
            "param_name={param_name}: flag must be set"
        );
    }
}

/// A tuple at exactly the limit (`MAX_REPRESENTABLE_TUPLE_LENGTH` elements total)
/// must NOT trigger the gate — the condition is strictly-greater-than.
#[test]
fn instantiate_tuple_at_exact_limit_does_not_trigger_gate() {
    let interner = TypeInterner::new();
    let _ = interner.take_tuple_too_large();

    // [T] where T = MAX/2 elements; two spreads = exactly MAX (not over).
    // Only works cleanly when MAX is even.
    let half = MAX_REPRESENTABLE_TUPLE_LENGTH / 2;
    let concrete = make_concrete_tuple(&interner, half);

    let param = make_param(&interner, "T");
    let body = make_double_spread_tuple(&interner, param);

    let atom = interner.intern_string("T");
    let subst = TypeSubstitution::single(atom, concrete);

    let result = instantiate_type(&interner, body, &subst);

    // Should NOT be ERROR and flag should NOT be set.
    assert_ne!(
        result,
        TypeId::ERROR,
        "exactly {MAX_REPRESENTABLE_TUPLE_LENGTH} elements must not trigger the gate"
    );
    assert!(
        !interner.take_tuple_too_large(),
        "flag must not be set at exactly the limit"
    );
}

/// A single spread one element above the limit must trigger the gate.
#[test]
fn instantiate_tuple_single_spread_just_over_limit_triggers_gate() {
    let interner = TypeInterner::new();
    let _ = interner.take_tuple_too_large();

    let over = MAX_REPRESENTABLE_TUPLE_LENGTH + 1;
    let concrete = make_concrete_tuple(&interner, over);

    let param = make_param(&interner, "T");
    // Single-spread body: [...T]
    let body = interner.tuple(vec![TupleElement {
        type_id: param,
        name: None,
        optional: false,
        rest: true,
    }]);

    let atom = interner.intern_string("T");
    let subst = TypeSubstitution::single(atom, concrete);

    let result = instantiate_type(&interner, body, &subst);

    assert_eq!(
        result,
        TypeId::ERROR,
        "single spread of {over} elements (> {MAX_REPRESENTABLE_TUPLE_LENGTH}) must trigger gate"
    );
    assert!(
        interner.take_tuple_too_large(),
        "flag must be set for single over-limit spread"
    );
}

/// Verify that the concrete tuple returned after substituting T has the
/// expected element count. This checks the correctness of the test harness
/// itself — if the concrete tuple's element count is wrong, the gate tests
/// would fail for the wrong reason.
#[test]
fn concrete_tuple_has_expected_length_after_instantiation() {
    let interner = TypeInterner::new();

    let expected_len = MAX_REPRESENTABLE_TUPLE_LENGTH + 1; // 10001
    let concrete = make_concrete_tuple(&interner, expected_len);

    // Directly check the concrete tuple's element count via lookup.
    if let Some(TypeData::Tuple(list_id)) = interner.lookup(concrete) {
        let elements = interner.tuple_list(list_id);
        assert_eq!(
            elements.len(),
            expected_len,
            "concrete tuple must have exactly {expected_len} elements"
        );
    } else {
        panic!("concrete tuple must resolve to TypeData::Tuple");
    }

    // Also verify it survives substitution with an identity-like substitution.
    let param = make_param(&interner, "T");
    let atom = interner.intern_string("T");
    let subst = TypeSubstitution::single(atom, concrete);
    // Instantiate just the type parameter (not a spread) to get the concrete tuple back.
    let result = instantiate_type(&interner, param, &subst);
    assert_eq!(
        result, concrete,
        "substituting T -> concrete_tuple must return concrete_tuple"
    );
    if let Some(TypeData::Tuple(list_id)) = interner.lookup(result) {
        let elements = interner.tuple_list(list_id);
        assert_eq!(
            elements.len(),
            expected_len,
            "after substitution, inner tuple must still have {expected_len} elements"
        );
    } else {
        panic!("result after substitution must be TypeData::Tuple");
    }
}

/// Verify gate arithmetic: 0 + 10001 = 10001 > 10000 fires the hard gate.
/// This is a pure arithmetic check of the gate condition, not a test of
/// the instantiation path, used to verify the test harness is correct.
#[test]
fn gate_arithmetic_fires_for_10001_elements() {
    let represented_len: usize = 0;
    let inner_len: usize = MAX_REPRESENTABLE_TUPLE_LENGTH + 1; // 10001
    let represented_after = represented_len.saturating_add(inner_len);
    assert!(
        represented_after > MAX_REPRESENTABLE_TUPLE_LENGTH,
        "{represented_after} must be > {MAX_REPRESENTABLE_TUPLE_LENGTH}"
    );
}

/// A direct (non-spread) tuple construction must not trigger the spread gate,
/// even when it has many elements.
#[test]
fn direct_tuple_construction_does_not_trigger_spread_gate() {
    let interner = TypeInterner::new();
    let _ = interner.take_tuple_too_large();

    // Build a large concrete tuple without going through instantiation.
    let large = MAX_REPRESENTABLE_TUPLE_LENGTH + 100;
    let tuple_id = make_concrete_tuple(&interner, large);

    // The gate must not have fired during tuple construction alone.
    assert!(
        !interner.take_tuple_too_large(),
        "direct tuple construction must not set flag"
    );
    // The result is not ERROR — it's a real (very large) Tuple type.
    assert_ne!(
        tuple_id,
        TypeId::ERROR,
        "direct construction must not return ERROR"
    );
}

/// `MAX_REPRESENTABLE_TUPLE_LENGTH` must equal `10_000` to match tsc's limit.
/// This test pins the documented constant so accidental changes are caught.
#[test]
fn max_tuple_length_matches_documented_constant() {
    assert_eq!(
        MAX_REPRESENTABLE_TUPLE_LENGTH, 10_000,
        "MAX_REPRESENTABLE_TUPLE_LENGTH must be 10,000 to match tsc's cardinality guard"
    );
}

/// The `tuple_too_large` flag follows a set-take-set lifecycle:
/// take clears, subsequent set re-fires, subsequent take clears again.
#[test]
fn tuple_too_large_flag_lifecycle_is_set_take_set() {
    let interner = TypeInterner::new();

    // Initially clear.
    assert!(!interner.take_tuple_too_large(), "flag should start clear");

    // Set it.
    interner.mark_tuple_too_large();
    assert!(
        interner.take_tuple_too_large(),
        "flag should be true after mark"
    );

    // Taking clears it.
    assert!(
        !interner.take_tuple_too_large(),
        "flag should be cleared after take"
    );

    // Can be set again.
    interner.mark_tuple_too_large();
    assert!(
        interner.take_tuple_too_large(),
        "flag can be re-set after clearing"
    );
}
