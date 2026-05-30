//! Tests for variadic tuple inference via `infer_from_types`.
//!
//! Structural rule: when inferring from a concrete source tuple against a
//! variadic target tuple, tsc aligns fixed elements from the front (prefix)
//! and from the back (suffix), then collects the middle source elements into a
//! tuple type that is inferred against the rest type parameter.

use super::*;
use crate::inference::infer::InferenceContext;
use crate::intern::TypeInterner;
use crate::types::{InferencePriority, TupleElement, TypeData, TypeParamInfo};

fn make_type_param(interner: &TypeInterner, name: &str) -> (tsz_common::interner::Atom, TypeId) {
    let atom = interner.intern_string(name);
    let ty = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: atom,
        constraint: None,
        default: None,
        is_const: false,
    }));
    (atom, ty)
}

fn fixed(type_id: TypeId) -> TupleElement {
    TupleElement {
        type_id,
        name: None,
        optional: false,
        rest: false,
    }
}

fn rest(type_id: TypeId) -> TupleElement {
    TupleElement {
        type_id,
        name: None,
        optional: false,
        rest: true,
    }
}

// =============================================================================
// Trailing-rest patterns: [H, ...Tail]
// =============================================================================

#[test]
fn infer_trailing_rest_one_prefix_element() {
    // Source: [string, number, boolean]   Target: [H, ...Tail]
    // → H = string, Tail = [number, boolean]
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let (h_name, h_type) = make_type_param(&interner, "H");
    let (tail_name, tail_type) = make_type_param(&interner, "Tail");
    ctx.fresh_type_param(h_name, false);
    ctx.fresh_type_param(tail_name, false);

    let source = interner.tuple(vec![
        fixed(TypeId::STRING),
        fixed(TypeId::NUMBER),
        fixed(TypeId::BOOLEAN),
    ]);
    let target = interner.tuple(vec![fixed(h_type), rest(tail_type)]);

    ctx.infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    let vh = ctx.find_type_param(h_name).unwrap();
    let vt = ctx.find_type_param(tail_name).unwrap();
    assert_eq!(
        ctx.resolve_with_constraints(vh).unwrap(),
        TypeId::STRING,
        "H should be string"
    );

    let expected_tail = interner.tuple(vec![fixed(TypeId::NUMBER), fixed(TypeId::BOOLEAN)]);
    assert_eq!(
        ctx.resolve_with_constraints(vt).unwrap(),
        expected_tail,
        "Tail should be [number, boolean]"
    );
}

#[test]
fn infer_trailing_rest_renamed_params() {
    // Same as above but with renamed type params (proves non-name-keyed)
    // Source: [string, number, boolean]   Target: [X, ...Y]
    // → X = string, Y = [number, boolean]
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let (x_name, x_type) = make_type_param(&interner, "X");
    let (y_name, y_type) = make_type_param(&interner, "Y");
    ctx.fresh_type_param(x_name, false);
    ctx.fresh_type_param(y_name, false);

    let source = interner.tuple(vec![
        fixed(TypeId::STRING),
        fixed(TypeId::NUMBER),
        fixed(TypeId::BOOLEAN),
    ]);
    let target = interner.tuple(vec![fixed(x_type), rest(y_type)]);

    ctx.infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    let vx = ctx.find_type_param(x_name).unwrap();
    let vy = ctx.find_type_param(y_name).unwrap();
    assert_eq!(ctx.resolve_with_constraints(vx).unwrap(), TypeId::STRING);
    let expected = interner.tuple(vec![fixed(TypeId::NUMBER), fixed(TypeId::BOOLEAN)]);
    assert_eq!(ctx.resolve_with_constraints(vy).unwrap(), expected);
}

#[test]
fn infer_trailing_rest_empty_middle() {
    // Source: [string]   Target: [H, ...Tail]
    // → H = string, Tail = [] (empty tuple)
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let (h_name, h_type) = make_type_param(&interner, "H");
    let (tail_name, tail_type) = make_type_param(&interner, "Tail");
    ctx.fresh_type_param(h_name, false);
    ctx.fresh_type_param(tail_name, false);

    let source = interner.tuple(vec![fixed(TypeId::STRING)]);
    let target = interner.tuple(vec![fixed(h_type), rest(tail_type)]);

    ctx.infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    let vh = ctx.find_type_param(h_name).unwrap();
    let vt = ctx.find_type_param(tail_name).unwrap();
    assert_eq!(ctx.resolve_with_constraints(vh).unwrap(), TypeId::STRING);
    let expected_empty = interner.tuple(vec![]);
    assert_eq!(ctx.resolve_with_constraints(vt).unwrap(), expected_empty);
}

// =============================================================================
// Leading-rest patterns: [...Init, L]
// =============================================================================

#[test]
fn infer_leading_rest_one_suffix_element() {
    // Source: [string, number, boolean]   Target: [...Init, L]
    // → Init = [string, number], L = boolean
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let (init_name, init_type) = make_type_param(&interner, "Init");
    let (l_name, l_type) = make_type_param(&interner, "L");
    ctx.fresh_type_param(init_name, false);
    ctx.fresh_type_param(l_name, false);

    let source = interner.tuple(vec![
        fixed(TypeId::STRING),
        fixed(TypeId::NUMBER),
        fixed(TypeId::BOOLEAN),
    ]);
    let target = interner.tuple(vec![rest(init_type), fixed(l_type)]);

    ctx.infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    let vi = ctx.find_type_param(init_name).unwrap();
    let vl = ctx.find_type_param(l_name).unwrap();
    assert_eq!(
        ctx.resolve_with_constraints(vl).unwrap(),
        TypeId::BOOLEAN,
        "L should be boolean"
    );
    let expected_init = interner.tuple(vec![fixed(TypeId::STRING), fixed(TypeId::NUMBER)]);
    assert_eq!(
        ctx.resolve_with_constraints(vi).unwrap(),
        expected_init,
        "Init should be [string, number]"
    );
}

#[test]
fn infer_leading_rest_renamed_params() {
    // Source: [string, number, boolean]   Target: [...P, Q]
    // → P = [string, number], Q = boolean
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let (p_name, p_type) = make_type_param(&interner, "P");
    let (q_name, q_type) = make_type_param(&interner, "Q");
    ctx.fresh_type_param(p_name, false);
    ctx.fresh_type_param(q_name, false);

    let source = interner.tuple(vec![
        fixed(TypeId::STRING),
        fixed(TypeId::NUMBER),
        fixed(TypeId::BOOLEAN),
    ]);
    let target = interner.tuple(vec![rest(p_type), fixed(q_type)]);

    ctx.infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    let vp = ctx.find_type_param(p_name).unwrap();
    let vq = ctx.find_type_param(q_name).unwrap();
    assert_eq!(ctx.resolve_with_constraints(vq).unwrap(), TypeId::BOOLEAN);
    let expected = interner.tuple(vec![fixed(TypeId::STRING), fixed(TypeId::NUMBER)]);
    assert_eq!(ctx.resolve_with_constraints(vp).unwrap(), expected);
}

#[test]
fn infer_leading_rest_single_source_element() {
    // Source: [boolean]   Target: [...Init, L]
    // → Init = [], L = boolean
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let (init_name, init_type) = make_type_param(&interner, "Init");
    let (l_name, l_type) = make_type_param(&interner, "L");
    ctx.fresh_type_param(init_name, false);
    ctx.fresh_type_param(l_name, false);

    let source = interner.tuple(vec![fixed(TypeId::BOOLEAN)]);
    let target = interner.tuple(vec![rest(init_type), fixed(l_type)]);

    ctx.infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    let vi = ctx.find_type_param(init_name).unwrap();
    let vl = ctx.find_type_param(l_name).unwrap();
    assert_eq!(ctx.resolve_with_constraints(vl).unwrap(), TypeId::BOOLEAN);
    let expected_empty = interner.tuple(vec![]);
    assert_eq!(ctx.resolve_with_constraints(vi).unwrap(), expected_empty);
}

// =============================================================================
// Fixed-prefix + rest + fixed-suffix: [H, ...Mid, L]
// =============================================================================

#[test]
fn infer_prefix_rest_suffix() {
    // Source: [string, number, boolean, bigint]  Target: [H, ...Mid, L]
    // → H = string, Mid = [number, boolean], L = bigint
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let (h_name, h_type) = make_type_param(&interner, "H");
    let (mid_name, mid_type) = make_type_param(&interner, "Mid");
    let (l_name, l_type) = make_type_param(&interner, "L");
    ctx.fresh_type_param(h_name, false);
    ctx.fresh_type_param(mid_name, false);
    ctx.fresh_type_param(l_name, false);

    let source = interner.tuple(vec![
        fixed(TypeId::STRING),
        fixed(TypeId::NUMBER),
        fixed(TypeId::BOOLEAN),
        fixed(TypeId::BIGINT),
    ]);
    let target = interner.tuple(vec![fixed(h_type), rest(mid_type), fixed(l_type)]);

    ctx.infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    let vh = ctx.find_type_param(h_name).unwrap();
    let vm = ctx.find_type_param(mid_name).unwrap();
    let vl = ctx.find_type_param(l_name).unwrap();
    assert_eq!(ctx.resolve_with_constraints(vh).unwrap(), TypeId::STRING);
    assert_eq!(ctx.resolve_with_constraints(vl).unwrap(), TypeId::BIGINT);
    let expected_mid = interner.tuple(vec![fixed(TypeId::NUMBER), fixed(TypeId::BOOLEAN)]);
    assert_eq!(ctx.resolve_with_constraints(vm).unwrap(), expected_mid);
}

// =============================================================================
// Both sides have rest elements
// =============================================================================

#[test]
fn infer_rest_to_rest_single() {
    // Source: [...A]   Target: [...B]
    // → B = A (rest-to-rest maps type-to-type)
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let (a_name, a_type) = make_type_param(&interner, "A");
    let (b_name, b_type) = make_type_param(&interner, "B");
    ctx.fresh_type_param(a_name, false);
    ctx.fresh_type_param(b_name, false);

    let source = interner.tuple(vec![rest(a_type)]);
    let target = interner.tuple(vec![rest(b_type)]);

    ctx.infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    let vb = ctx.find_type_param(b_name).unwrap();
    assert_eq!(ctx.resolve_with_constraints(vb).unwrap(), a_type);
}

// =============================================================================
// Negative / fallback cases
// =============================================================================

#[test]
fn infer_no_rest_preserves_zip_behavior() {
    // Source: [string, number]   Target: [T, U]
    // → T = string, U = number  (no variadic elements; falls through to zip)
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);
    let (t_name, t_type) = make_type_param(&interner, "T");
    let (u_name, u_type) = make_type_param(&interner, "U");
    ctx.fresh_type_param(t_name, false);
    ctx.fresh_type_param(u_name, false);

    let source = interner.tuple(vec![fixed(TypeId::STRING), fixed(TypeId::NUMBER)]);
    let target = interner.tuple(vec![fixed(t_type), fixed(u_type)]);

    ctx.infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    let vt = ctx.find_type_param(t_name).unwrap();
    let vu = ctx.find_type_param(u_name).unwrap();
    assert_eq!(ctx.resolve_with_constraints(vt).unwrap(), TypeId::STRING);
    assert_eq!(ctx.resolve_with_constraints(vu).unwrap(), TypeId::NUMBER);
}
