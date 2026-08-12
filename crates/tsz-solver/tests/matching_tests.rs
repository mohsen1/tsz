//! Tests for structural type matching (inference via `infer_from_types`).
//!
//! These tests exercise the `infer_from_types` method on `InferenceContext`,
//! which is the core algorithm for inferring type parameters from function
//! arguments by walking type structures in parallel.

use super::*;
use crate::construction::TypeDatabase;
use crate::def::DefId;
use crate::inference::infer::{InferenceContext, InferenceError, ParameterRecoveryMode};
use crate::intern::TypeInterner;
use crate::relations::subtype::TypeResolver;
use crate::types::{
    CallSignature, CallableShape, FunctionShape, InferencePriority, ParamInfo, PropertyInfo,
    SymbolRef, TupleElement, TypeData, TypeParamInfo,
};

// =============================================================================
// Helper to create a TypeParameter type
// =============================================================================

fn make_type_param(interner: &TypeInterner, name: &str) -> (tsz_common::interner::Atom, TypeId) {
    let atom = interner.intern_string(name);
    let ty = interner.intern(TypeData::TypeParameter(TypeParamInfo {
        name: atom,
        constraint: None,
        default: None,
        is_const: false,
        origin: crate::types::TypeParamOrigin::User,
    }));
    (atom, ty)
}

fn unary_function(
    interner: &TypeInterner,
    parameter_type: TypeId,
    is_method: bool,
    is_constructor: bool,
) -> TypeId {
    interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            suppress_display_optional: false,
            name: Some(interner.intern_string("value")),
            type_id: parameter_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor,
        is_method,
    })
}

fn unary_call_signature(
    interner: &TypeInterner,
    parameter_type: TypeId,
    is_method: bool,
) -> CallSignature {
    CallSignature {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            suppress_display_optional: false,
            name: Some(interner.intern_string("entry")),
            type_id: parameter_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_method,
    }
}

fn function_with_this(interner: &TypeInterner, this_type: TypeId, is_method: bool) -> TypeId {
    interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: Some(this_type),
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method,
    })
}

fn assert_candidate_partition(
    context: &mut InferenceContext<'_>,
    variable: crate::inference::infer::InferenceVar,
    expected_regular: &[TypeId],
    expected_contra: &[TypeId],
) {
    let regular = context
        .get_constraints(variable)
        .map(|constraints| constraints.lower_bounds)
        .unwrap_or_default();
    assert_eq!(regular, expected_regular);
    assert_eq!(
        context.get_contra_candidate_types(variable),
        expected_contra
    );
}

struct CanonicalApplicationResolver {
    left: DefId,
    right: DefId,
    canonical: DefId,
}

impl TypeResolver for CanonicalApplicationResolver {
    fn resolve_ref(&self, _symbol: SymbolRef, _interner: &dyn TypeDatabase) -> Option<TypeId> {
        None
    }

    fn canonical_def_id(&self, def_id: DefId) -> DefId {
        if def_id == self.left || def_id == self.right {
            self.canonical
        } else {
            def_id
        }
    }

    fn defs_are_equivalent(&self, a: DefId, b: DefId) -> bool {
        a == b
    }
}

// =============================================================================
// Simple Matching: T against a concrete type
// =============================================================================

#[test]
fn test_match_number_against_t() {
    // Match `number` against `T` => infers T = number
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, t_type) = make_type_param(&interner, "T");
    let _var_t = ctx.fresh_type_param(t_name, false);

    ctx.infer_from_types(TypeId::NUMBER, t_type, InferencePriority::NakedTypeVariable)
        .unwrap();

    let var_t = ctx.find_type_param(t_name).unwrap();
    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::NUMBER);
}

#[test]
fn test_match_string_against_t() {
    // Match `string` against `T` => infers T = string
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, t_type) = make_type_param(&interner, "T");
    let _var_t = ctx.fresh_type_param(t_name, false);

    ctx.infer_from_types(TypeId::STRING, t_type, InferencePriority::NakedTypeVariable)
        .unwrap();

    let var_t = ctx.find_type_param(t_name).unwrap();
    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_match_literal_against_t() {
    // Match `"hello"` against `T` => infers T = "hello" (literal string)
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, t_type) = make_type_param(&interner, "T");
    let _var_t = ctx.fresh_type_param(t_name, false);

    let hello = interner.literal_string("hello");
    ctx.infer_from_types(hello, t_type, InferencePriority::NakedTypeVariable)
        .unwrap();

    let var_t = ctx.find_type_param(t_name).unwrap();
    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // Fresh literal should widen to string
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_match_boolean_against_t() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, t_type) = make_type_param(&interner, "T");
    let _var_t = ctx.fresh_type_param(t_name, false);

    ctx.infer_from_types(
        TypeId::BOOLEAN,
        t_type,
        InferencePriority::NakedTypeVariable,
    )
    .unwrap();

    let var_t = ctx.find_type_param(t_name).unwrap();
    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::BOOLEAN);
}

#[test]
fn test_match_same_type_no_inference() {
    // If source == target (same TypeId), no inference happens
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, _t_type) = make_type_param(&interner, "T");
    let _var_t = ctx.fresh_type_param(t_name, false);

    // Matching number against number should be a no-op
    ctx.infer_from_types(
        TypeId::NUMBER,
        TypeId::NUMBER,
        InferencePriority::NakedTypeVariable,
    )
    .unwrap();

    // T should remain unresolved since we didn't match against T
    let var_t = ctx.find_type_param(t_name).unwrap();
    let result = ctx.probe(var_t);
    assert!(result.is_none());
}

// =============================================================================
// Object Matching
// =============================================================================

#[test]
fn test_match_object_property() {
    // Match `{ x: string }` against `{ x: T }` => infers T = string
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, t_type) = make_type_param(&interner, "T");
    let _var_t = ctx.fresh_type_param(t_name, false);

    let name_x = interner.intern_string("x");
    let source = interner.object(vec![PropertyInfo::new(name_x, TypeId::STRING)]);
    let target = interner.object(vec![PropertyInfo::new(name_x, t_type)]);

    ctx.infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    let var_t = ctx.find_type_param(t_name).unwrap();
    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_match_object_multiple_properties() {
    // Match `{ x: string, y: number }` against `{ x: T, y: U }`
    // => infers T = string, U = number
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, t_type) = make_type_param(&interner, "T");
    let (u_name, u_type) = make_type_param(&interner, "U");
    let _var_t = ctx.fresh_type_param(t_name, false);
    let _var_u = ctx.fresh_type_param(u_name, false);

    let name_x = interner.intern_string("x");
    let name_y = interner.intern_string("y");

    let source = interner.object(vec![
        PropertyInfo::new(name_x, TypeId::STRING),
        PropertyInfo::new(name_y, TypeId::NUMBER),
    ]);
    let target = interner.object(vec![
        PropertyInfo::new(name_x, t_type),
        PropertyInfo::new(name_y, u_type),
    ]);

    ctx.infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    let var_t = ctx.find_type_param(t_name).unwrap();
    let var_u = ctx.find_type_param(u_name).unwrap();
    let result_t = ctx.resolve_with_constraints(var_t).unwrap();
    let result_u = ctx.resolve_with_constraints(var_u).unwrap();

    assert_eq!(result_t, TypeId::STRING);
    assert_eq!(result_u, TypeId::NUMBER);
}

#[test]
fn test_match_object_extra_source_properties() {
    // Match `{ x: string, y: number, z: boolean }` against `{ x: T }`
    // => infers T = string (extra props are ignored)
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, t_type) = make_type_param(&interner, "T");
    let _var_t = ctx.fresh_type_param(t_name, false);

    let name_x = interner.intern_string("x");
    let name_y = interner.intern_string("y");
    let name_z = interner.intern_string("z");

    let source = interner.object(vec![
        PropertyInfo::new(name_x, TypeId::STRING),
        PropertyInfo::new(name_y, TypeId::NUMBER),
        PropertyInfo::new(name_z, TypeId::BOOLEAN),
    ]);
    let target = interner.object(vec![PropertyInfo::new(name_x, t_type)]);

    ctx.infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    let var_t = ctx.find_type_param(t_name).unwrap();
    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_match_intersection_naked_param_uses_corresponding_source_member() {
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, t_type) = make_type_param(&interner, "T");
    let (p_name, p_type) = make_type_param(&interner, "P");
    let _var_t = ctx.fresh_type_param(t_name, false);
    let _var_p = ctx.fresh_type_param(p_name, false);

    let marker_name = interner.intern_string("marker");
    let props_name = interner.intern_string("props");
    let source_marker = interner.object(vec![PropertyInfo::new(marker_name, TypeId::STRING)]);
    let source_props = interner.object(vec![PropertyInfo::new(props_name, TypeId::NUMBER)]);
    let source = interner.intersect_types_raw(vec![source_marker, source_props]);
    let target_props = interner.object(vec![PropertyInfo::new(props_name, p_type)]);
    let target = interner.intersection(vec![t_type, target_props]);

    ctx.infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    let var_t = ctx.find_type_param(t_name).unwrap();
    let var_p = ctx.find_type_param(p_name).unwrap();
    let result_t = ctx.resolve_with_constraints(var_t).unwrap();
    let result_p = ctx.resolve_with_constraints(var_p).unwrap();

    assert_eq!(
        result_t, source_marker,
        "naked intersection member should infer from the corresponding source member"
    );
    assert_eq!(result_p, TypeId::NUMBER);
}

#[test]
fn test_match_object_missing_source_property() {
    // Match `{ x: string }` against `{ x: T, y: U }`
    // => infers T = string, U stays unresolved
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, t_type) = make_type_param(&interner, "T");
    let (u_name, u_type) = make_type_param(&interner, "U");
    let _var_t = ctx.fresh_type_param(t_name, false);
    let _var_u = ctx.fresh_type_param(u_name, false);

    let name_x = interner.intern_string("x");
    let name_y = interner.intern_string("y");

    let source = interner.object(vec![PropertyInfo::new(name_x, TypeId::STRING)]);
    let target = interner.object(vec![
        PropertyInfo::new(name_x, t_type),
        PropertyInfo::new(name_y, u_type),
    ]);

    ctx.infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    let var_t = ctx.find_type_param(t_name).unwrap();
    let var_u = ctx.find_type_param(u_name).unwrap();

    let result_t = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result_t, TypeId::STRING);

    // U should be unresolved
    let result_u = ctx.probe(var_u);
    assert!(result_u.is_none());
}

// =============================================================================
// Function Matching
// =============================================================================

#[test]
fn test_match_function_param_and_return() {
    // Match `(n: number) => string` against `(x: T) => U`
    // => infers T = number (contravariant), U = string (covariant)
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, t_type) = make_type_param(&interner, "T");
    let (u_name, u_type) = make_type_param(&interner, "U");
    let _var_t = ctx.fresh_type_param(t_name, false);
    let _var_u = ctx.fresh_type_param(u_name, false);

    let source = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            suppress_display_optional: false,
            name: Some(interner.intern_string("n")),
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::STRING,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let target = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            suppress_display_optional: false,
            name: Some(interner.intern_string("x")),
            type_id: t_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: u_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    ctx.infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    let var_t = ctx.find_type_param(t_name).unwrap();
    let var_u = ctx.find_type_param(u_name).unwrap();

    // Parameters are contravariant: the inference walks target<->source swapped,
    // so T gets an upper bound of number
    let result_t = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result_t, TypeId::NUMBER);

    let result_u = ctx.resolve_with_constraints(var_u).unwrap();
    assert_eq!(result_u, TypeId::STRING);
}

#[test]
fn test_match_function_multiple_params() {
    // Match `(a: string, b: number) => boolean` against `(x: T, y: U) => V`
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, t_type) = make_type_param(&interner, "T");
    let (u_name, u_type) = make_type_param(&interner, "U");
    let (v_name, v_type) = make_type_param(&interner, "V");
    let _var_t = ctx.fresh_type_param(t_name, false);
    let _var_u = ctx.fresh_type_param(u_name, false);
    let _var_v = ctx.fresh_type_param(v_name, false);

    let source = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![
            ParamInfo {
                suppress_display_optional: false,
                name: Some(interner.intern_string("a")),
                type_id: TypeId::STRING,
                optional: false,
                rest: false,
            },
            ParamInfo {
                suppress_display_optional: false,
                name: Some(interner.intern_string("b")),
                type_id: TypeId::NUMBER,
                optional: false,
                rest: false,
            },
        ],
        this_type: None,
        return_type: TypeId::BOOLEAN,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let target = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![
            ParamInfo {
                suppress_display_optional: false,
                name: Some(interner.intern_string("x")),
                type_id: t_type,
                optional: false,
                rest: false,
            },
            ParamInfo {
                suppress_display_optional: false,
                name: Some(interner.intern_string("y")),
                type_id: u_type,
                optional: false,
                rest: false,
            },
        ],
        this_type: None,
        return_type: v_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    ctx.infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    let var_t = ctx.find_type_param(t_name).unwrap();
    let var_u = ctx.find_type_param(u_name).unwrap();
    let var_v = ctx.find_type_param(v_name).unwrap();

    assert_eq!(ctx.resolve_with_constraints(var_t).unwrap(), TypeId::STRING);
    assert_eq!(ctx.resolve_with_constraints(var_u).unwrap(), TypeId::NUMBER);
    assert_eq!(
        ctx.resolve_with_constraints(var_v).unwrap(),
        TypeId::BOOLEAN
    );
}

#[test]
fn test_match_function_return_only() {
    // Match `() => number` against `() => T` => T = number
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, t_type) = make_type_param(&interner, "T");
    let _var_t = ctx.fresh_type_param(t_name, false);

    let source = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let target = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: None,
        return_type: t_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    ctx.infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    let var_t = ctx.find_type_param(t_name).unwrap();
    assert_eq!(ctx.resolve_with_constraints(var_t).unwrap(), TypeId::NUMBER);
}

// =============================================================================
// Array Matching
// =============================================================================

#[test]
fn test_match_array_element_type() {
    // Match `number[]` against `T[]` => infers T = number
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, t_type) = make_type_param(&interner, "T");
    let _var_t = ctx.fresh_type_param(t_name, false);

    let source = interner.array(TypeId::NUMBER);
    let target = interner.array(t_type);

    ctx.infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    let var_t = ctx.find_type_param(t_name).unwrap();
    assert_eq!(ctx.resolve_with_constraints(var_t).unwrap(), TypeId::NUMBER);
}

#[test]
fn test_match_array_of_objects() {
    // Match `{ x: string }[]` against `T[]` => T = { x: string }
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, t_type) = make_type_param(&interner, "T");
    let _var_t = ctx.fresh_type_param(t_name, false);

    let name_x = interner.intern_string("x");
    let obj = interner.object(vec![PropertyInfo::new(name_x, TypeId::STRING)]);

    let source = interner.array(obj);
    let target = interner.array(t_type);

    ctx.infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    let var_t = ctx.find_type_param(t_name).unwrap();
    assert_eq!(ctx.resolve_with_constraints(var_t).unwrap(), obj);
}

// =============================================================================
// Tuple Matching
// =============================================================================

#[test]
fn test_match_tuple_elements() {
    // Match `[string, number]` against `[T, U]`
    // => T = string, U = number
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, t_type) = make_type_param(&interner, "T");
    let (u_name, u_type) = make_type_param(&interner, "U");
    let _var_t = ctx.fresh_type_param(t_name, false);
    let _var_u = ctx.fresh_type_param(u_name, false);

    let source = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    let target = interner.tuple(vec![
        TupleElement {
            type_id: t_type,
            name: None,
            optional: false,
            rest: false,
        },
        TupleElement {
            type_id: u_type,
            name: None,
            optional: false,
            rest: false,
        },
    ]);

    ctx.infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    let var_t = ctx.find_type_param(t_name).unwrap();
    let var_u = ctx.find_type_param(u_name).unwrap();

    assert_eq!(ctx.resolve_with_constraints(var_t).unwrap(), TypeId::STRING);
    assert_eq!(ctx.resolve_with_constraints(var_u).unwrap(), TypeId::NUMBER);
}

#[test]
fn test_match_tuple_single_element() {
    // Match `[boolean]` against `[T]`
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, t_type) = make_type_param(&interner, "T");
    let _var_t = ctx.fresh_type_param(t_name, false);

    let source = interner.tuple(vec![TupleElement {
        type_id: TypeId::BOOLEAN,
        name: None,
        optional: false,
        rest: false,
    }]);

    let target = interner.tuple(vec![TupleElement {
        type_id: t_type,
        name: None,
        optional: false,
        rest: false,
    }]);

    ctx.infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    let var_t = ctx.find_type_param(t_name).unwrap();
    assert_eq!(
        ctx.resolve_with_constraints(var_t).unwrap(),
        TypeId::BOOLEAN
    );
}

// =============================================================================
// Union Matching
// =============================================================================

#[test]
fn test_match_source_union_against_target_union() {
    // Match `string | number` against `T | U`
    // The union-to-union inference should handle this
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, t_type) = make_type_param(&interner, "T");
    let _var_t = ctx.fresh_type_param(t_name, false);

    let source = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);

    // Target is just T (not a union of params) - source union against T
    ctx.infer_from_types(source, t_type, InferencePriority::NakedTypeVariable)
        .unwrap();

    let var_t = ctx.find_type_param(t_name).unwrap();
    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // T should be inferred as string | number
    assert_eq!(result, source);
}

#[test]
fn test_match_against_union_target_with_fixed_members() {
    // Match `string` against `T | undefined`
    // T should be inferred as string (undefined is fixed)
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, t_type) = make_type_param(&interner, "T");
    let _var_t = ctx.fresh_type_param(t_name, false);

    let target = interner.union(vec![t_type, TypeId::UNDEFINED]);

    ctx.infer_from_types(TypeId::STRING, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    let var_t = ctx.find_type_param(t_name).unwrap();
    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

// =============================================================================
// Nested Matching
// =============================================================================

#[test]
fn test_match_nested_array_in_object() {
    // Match `{ items: number[] }` against `{ items: T[] }`
    // => T = number
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, t_type) = make_type_param(&interner, "T");
    let _var_t = ctx.fresh_type_param(t_name, false);

    let name_items = interner.intern_string("items");

    let source = interner.object(vec![PropertyInfo::new(
        name_items,
        interner.array(TypeId::NUMBER),
    )]);
    let target = interner.object(vec![PropertyInfo::new(name_items, interner.array(t_type))]);

    ctx.infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    let var_t = ctx.find_type_param(t_name).unwrap();
    assert_eq!(ctx.resolve_with_constraints(var_t).unwrap(), TypeId::NUMBER);
}

#[test]
fn test_match_nested_object_in_object() {
    // Match `{ inner: { value: boolean } }` against `{ inner: { value: T } }`
    // => T = boolean
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, t_type) = make_type_param(&interner, "T");
    let _var_t = ctx.fresh_type_param(t_name, false);

    let name_inner = interner.intern_string("inner");
    let name_value = interner.intern_string("value");

    let inner_source = interner.object(vec![PropertyInfo::new(name_value, TypeId::BOOLEAN)]);
    let source = interner.object(vec![PropertyInfo::new(name_inner, inner_source)]);

    let inner_target = interner.object(vec![PropertyInfo::new(name_value, t_type)]);
    let target = interner.object(vec![PropertyInfo::new(name_inner, inner_target)]);

    ctx.infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    let var_t = ctx.find_type_param(t_name).unwrap();
    assert_eq!(
        ctx.resolve_with_constraints(var_t).unwrap(),
        TypeId::BOOLEAN
    );
}

#[test]
fn test_match_function_returning_array() {
    // Match `() => string[]` against `() => T[]` => T = string
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, t_type) = make_type_param(&interner, "T");
    let _var_t = ctx.fresh_type_param(t_name, false);

    let source = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: None,
        return_type: interner.array(TypeId::STRING),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let target = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: Vec::new(),
        this_type: None,
        return_type: interner.array(t_type),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    ctx.infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    let var_t = ctx.find_type_param(t_name).unwrap();
    assert_eq!(ctx.resolve_with_constraints(var_t).unwrap(), TypeId::STRING);
}

// =============================================================================
// Contravariant Matching
// =============================================================================

#[test]
fn test_match_contravariant_parameter() {
    // In function parameter position, inference is contravariant.
    // Match `(x: string) => void` against `(x: T) => void`
    // T gets an upper bound from contravariant position.
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, t_type) = make_type_param(&interner, "T");
    let _var_t = ctx.fresh_type_param(t_name, false);

    let source = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            suppress_display_optional: false,
            name: Some(interner.intern_string("x")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let target = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            suppress_display_optional: false,
            name: Some(interner.intern_string("x")),
            type_id: t_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    ctx.infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    let var_t = ctx.find_type_param(t_name).unwrap();
    // In contravariant position, T gets string as an upper bound
    // (because infer_functions swaps target and source for params)
    let result = ctx.resolve_with_constraints(var_t).unwrap();
    assert_eq!(result, TypeId::STRING);
}

#[test]
fn test_method_target_parameter_records_regular_candidate_with_renamed_binders() {
    for binder in ["Payload", "RenamedItem"] {
        let interner = TypeInterner::new();
        let mut context = InferenceContext::new(&interner);
        let (parameter_name, parameter_type) = make_type_param(&interner, binder);
        let variable = context.fresh_type_param(parameter_name, false);

        let source = unary_function(&interner, TypeId::STRING, false, false);
        let target = unary_function(&interner, parameter_type, true, false);
        context
            .infer_from_types(source, target, InferencePriority::NakedTypeVariable)
            .unwrap();

        assert_candidate_partition(&mut context, variable, &[TypeId::STRING], &[]);
    }
}

#[test]
fn test_constructor_inference_uses_target_declaration_kind() {
    for (binder, target_is_class, regular, contra) in [
        ("ClassConstructor", true, &[TypeId::NUMBER][..], &[][..]),
        ("ConstructType", false, &[][..], &[TypeId::NUMBER][..]),
    ] {
        let interner = TypeInterner::new();
        let mut context = InferenceContext::new(&interner);
        let (parameter_name, parameter_type) = make_type_param(&interner, binder);
        let variable = context.fresh_type_param(parameter_name, false);
        let source = unary_function(&interner, TypeId::NUMBER, true, true);
        let target = unary_function(&interner, parameter_type, target_is_class, true);

        context
            .infer_from_types(source, target, InferencePriority::NakedTypeVariable)
            .unwrap();

        assert_candidate_partition(&mut context, variable, regular, contra);
    }
}

#[test]
fn test_method_property_hint_does_not_loosen_strict_constructor_target() {
    let interner = TypeInterner::new();
    let (name, target_param) = make_type_param(&interner, "Constructed");
    let source = unary_function(&interner, TypeId::STRING, true, true);
    let target = unary_function(&interner, target_param, false, true);
    let mut context = InferenceContext::new(&interner);
    let variable = context.fresh_type_param(name, false);
    context.pending_target_method = true;

    context
        .infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .expect("constructor inference should complete");

    assert_candidate_partition(&mut context, variable, &[], &[TypeId::STRING]);
    assert!(context.pending_target_method);
}

#[test]
fn test_method_property_hint_does_not_reach_returned_signature() {
    let interner = TypeInterner::new();
    let (name, target_param) = make_type_param(&interner, "Returned");
    let source_return = unary_function(&interner, TypeId::STRING, false, false);
    let target_return = unary_function(&interner, target_param, false, false);
    let source = interner.function(FunctionShape::new(Vec::new(), source_return));
    let target = interner.function(FunctionShape::new(Vec::new(), target_return));
    let mut context = InferenceContext::new(&interner);
    let variable = context.fresh_type_param(name, false);
    context.pending_target_method = true;

    context
        .infer_from_types(source, target, InferencePriority::ReturnType)
        .expect("nested return inference should complete");

    assert_candidate_partition(&mut context, variable, &[], &[TypeId::STRING]);
    assert!(context.pending_target_method);
}

#[test]
fn test_method_property_metadata_reaches_rebuilt_function_signature() {
    let interner = TypeInterner::new();
    let mut context = InferenceContext::new(&interner);
    let (parameter_name, parameter_type) = make_type_param(&interner, "Element");
    let variable = context.fresh_type_param(parameter_name, false);
    let member_name = interner.intern_string("consume");

    let source_member = unary_function(&interner, TypeId::BOOLEAN, false, false);
    let target_member = unary_function(&interner, parameter_type, false, false);
    let source = interner.object(vec![PropertyInfo::new(member_name, source_member)]);
    let mut target_property = PropertyInfo::new(member_name, target_member);
    target_property.is_method = true;
    let target = interner.object(vec![target_property]);

    context
        .infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    assert_candidate_partition(&mut context, variable, &[TypeId::BOOLEAN], &[]);
}

#[test]
fn test_method_bivariant_mode_persists_through_nested_callback_signature() {
    let interner = TypeInterner::new();
    let mut context = InferenceContext::new(&interner);
    let (parameter_name, parameter_type) = make_type_param(&interner, "NestedValue");
    let variable = context.fresh_type_param(parameter_name, false);

    let source_callback = unary_function(&interner, TypeId::STRING, false, false);
    let target_callback = unary_function(&interner, parameter_type, false, false);
    let source_method = unary_function(&interner, source_callback, false, false);
    let target_method = unary_function(&interner, target_callback, true, false);

    context
        .infer_from_types(
            source_method,
            target_method,
            InferencePriority::NakedTypeVariable,
        )
        .unwrap();

    assert_candidate_partition(&mut context, variable, &[TypeId::STRING], &[]);
}

#[test]
fn test_nested_strict_function_parameters_toggle_back_to_covariance() {
    let interner = TypeInterner::new();
    let mut context = InferenceContext::new(&interner);
    let (parameter_name, parameter_type) = make_type_param(&interner, "DoubleContra");
    let variable = context.fresh_type_param(parameter_name, false);
    let source_inner = unary_function(&interner, TypeId::STRING, false, false);
    let target_inner = unary_function(&interner, parameter_type, false, false);
    let source = unary_function(&interner, source_inner, false, false);
    let target = unary_function(&interner, target_inner, false, false);

    context
        .infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    assert_candidate_partition(&mut context, variable, &[TypeId::STRING], &[]);
}

#[test]
fn test_nested_variance_walk_keeps_source_placeholder_as_regular_candidate() {
    let interner = TypeInterner::new();
    let mut context = InferenceContext::new(&interner);
    let (parameter_name, parameter_type) = make_type_param(&interner, "NestedSource");
    let variable = context.fresh_type_param(parameter_name, false);
    let source_inner = unary_function(&interner, parameter_type, false, false);
    let target_inner = unary_function(&interner, TypeId::STRING, false, false);
    let source = unary_function(&interner, source_inner, false, false);
    let target = unary_function(&interner, target_inner, false, false);

    context
        .infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    let constraints = context
        .get_constraints(variable)
        .expect("nested source placeholder should remain inference evidence");
    assert_eq!(constraints.lower_bounds, vec![TypeId::STRING]);
    assert!(constraints.upper_bounds.is_empty());
    assert!(context.get_contra_candidate_types(variable).is_empty());
}

#[test]
fn test_explicit_this_uses_target_method_variance() {
    for (binder, is_method, regular, contra) in [
        ("MethodThis", true, &[TypeId::STRING][..], &[][..]),
        ("PropertyThis", false, &[][..], &[TypeId::STRING][..]),
    ] {
        let interner = TypeInterner::new();
        let mut context = InferenceContext::new(&interner);
        let (parameter_name, parameter_type) = make_type_param(&interner, binder);
        let variable = context.fresh_type_param(parameter_name, false);
        let source = function_with_this(&interner, TypeId::STRING, false);
        let target = function_with_this(&interner, parameter_type, is_method);

        context
            .infer_from_types(source, target, InferencePriority::NakedTypeVariable)
            .unwrap();

        assert_candidate_partition(&mut context, variable, regular, contra);
    }
}

#[test]
fn test_function_valued_property_parameter_remains_contravariant() {
    let interner = TypeInterner::new();
    let mut context = InferenceContext::new(&interner);
    let (parameter_name, parameter_type) = make_type_param(&interner, "OrdinaryProperty");
    let variable = context.fresh_type_param(parameter_name, false);
    let member_name = interner.intern_string("callback");

    let source_member = unary_function(&interner, TypeId::NUMBER, false, false);
    let target_member = unary_function(&interner, parameter_type, false, false);
    let source = interner.object(vec![PropertyInfo::new(member_name, source_member)]);
    let target = interner.object(vec![PropertyInfo::new(member_name, target_member)]);

    context
        .infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    assert_candidate_partition(&mut context, variable, &[], &[TypeId::NUMBER]);
}

#[test]
fn test_callable_signature_inference_uses_target_declaration_kind() {
    for (binder, use_construct_signature, target_is_method) in [
        ("MethodArg", false, true),
        ("ClassCtorArg", true, true),
        ("ConstructTypeArg", true, false),
    ] {
        let interner = TypeInterner::new();
        let mut context = InferenceContext::new(&interner);
        let (parameter_name, parameter_type) = make_type_param(&interner, binder);
        let variable = context.fresh_type_param(parameter_name, false);

        let source_signature = unary_call_signature(&interner, TypeId::STRING, false);
        let target_signature = unary_call_signature(&interner, parameter_type, target_is_method);
        let (source_calls, source_constructs, target_calls, target_constructs) =
            if use_construct_signature {
                (
                    Vec::new(),
                    vec![source_signature],
                    Vec::new(),
                    vec![target_signature],
                )
            } else {
                (
                    vec![source_signature],
                    Vec::new(),
                    vec![target_signature],
                    Vec::new(),
                )
            };
        let source = interner.callable(CallableShape {
            call_signatures: source_calls,
            construct_signatures: source_constructs,
            ..CallableShape::default()
        });
        let target = interner.callable(CallableShape {
            call_signatures: target_calls,
            construct_signatures: target_constructs,
            ..CallableShape::default()
        });

        context
            .infer_from_types(source, target, InferencePriority::NakedTypeVariable)
            .unwrap();

        if target_is_method {
            assert_candidate_partition(&mut context, variable, &[TypeId::STRING], &[]);
        } else {
            assert_candidate_partition(&mut context, variable, &[], &[TypeId::STRING]);
        }
    }
}

#[test]
fn test_inference_modes_restore_after_error() {
    let interner = TypeInterner::new();
    let mut context = InferenceContext::new(&interner);
    let failed: Result<(), InferenceError> = context.with_restored_inference_modes(|ctx| {
        ctx.in_contra_mode = true;
        ctx.in_variance_walk = true;
        ctx.parameter_recovery_mode = ParameterRecoveryMode::ComplexPlaceholder;
        ctx.in_bivariant_mode = true;
        ctx.pending_target_method = true;
        Err(InferenceError::Conflict(TypeId::STRING, TypeId::NUMBER))
    });

    assert!(matches!(failed, Err(InferenceError::Conflict(..))));
    assert!(!context.in_contra_mode);
    assert!(!context.in_variance_walk);
    assert_eq!(context.parameter_recovery_mode, ParameterRecoveryMode::None);
    assert!(!context.in_bivariant_mode);
    assert!(!context.pending_target_method);
}

#[test]
fn test_constraint_visit_mode_omits_matcher_only_variance_state() {
    let interner = TypeInterner::new();
    let mut context = InferenceContext::new(&interner);
    let base_constraint_mode = context.constraint_visit_mode();
    let base_matching_mode = context.inference_visit_mode();

    context.in_variance_walk = true;
    assert_eq!(context.constraint_visit_mode(), base_constraint_mode);
    assert_ne!(context.inference_visit_mode(), base_matching_mode);

    context.parameter_recovery_mode = ParameterRecoveryMode::StandaloneReverse;
    assert_ne!(context.constraint_visit_mode(), base_constraint_mode);
}

#[test]
fn test_inference_visited_distinguishes_method_property_mode() {
    let interner = TypeInterner::new();
    let mut context = InferenceContext::new(&interner);
    let (parameter_name, parameter_type) = make_type_param(&interner, "VisitedMode");
    let variable = context.fresh_type_param(parameter_name, false);
    let ordinary_name = interner.intern_string("ordinary");
    let method_name = interner.intern_string("method");

    let source_member = unary_function(&interner, TypeId::BOOLEAN, false, false);
    let target_member = unary_function(&interner, parameter_type, false, false);
    let source = interner.object(vec![
        PropertyInfo::new(ordinary_name, source_member),
        PropertyInfo::new(method_name, source_member),
    ]);
    let mut method_property = PropertyInfo::new(method_name, target_member);
    method_property.is_method = true;
    let target = interner.object(vec![
        PropertyInfo::new(ordinary_name, target_member),
        method_property,
    ]);

    context
        .infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    assert_candidate_partition(
        &mut context,
        variable,
        &[TypeId::BOOLEAN],
        &[TypeId::BOOLEAN],
    );
}

// =============================================================================
// No Match Cases
// =============================================================================

#[test]
fn test_match_different_structures_no_panic() {
    // Match `string` against `{ x: T }` - no structural match, no panic
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, t_type) = make_type_param(&interner, "T");
    let _var_t = ctx.fresh_type_param(t_name, false);

    let name_x = interner.intern_string("x");
    let target = interner.object(vec![PropertyInfo::new(name_x, t_type)]);

    // String against object - no structural match
    ctx.infer_from_types(TypeId::STRING, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    // T should remain unresolved
    let var_t = ctx.find_type_param(t_name).unwrap();
    let result = ctx.probe(var_t);
    assert!(result.is_none());
}

#[test]
fn test_match_number_against_function_no_panic() {
    // Match `number` against `(x: T) => U` - no structural match
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, t_type) = make_type_param(&interner, "T");
    let (u_name, u_type) = make_type_param(&interner, "U");
    let _var_t = ctx.fresh_type_param(t_name, false);
    let _var_u = ctx.fresh_type_param(u_name, false);

    let target = interner.function(FunctionShape {
        type_params: Vec::new(),
        params: vec![ParamInfo {
            suppress_display_optional: false,
            name: Some(interner.intern_string("x")),
            type_id: t_type,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: u_type,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    ctx.infer_from_types(TypeId::NUMBER, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    // Both should remain unresolved
    let var_t = ctx.find_type_param(t_name).unwrap();
    let var_u = ctx.find_type_param(u_name).unwrap();
    assert!(ctx.probe(var_t).is_none());
    assert!(ctx.probe(var_u).is_none());
}

// =============================================================================
// Partial Match
// =============================================================================

#[test]
fn test_match_partial_object_properties() {
    // Match `{ x: string }` against `{ x: T, y: U }`
    // T should be inferred, U should remain unresolved
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, t_type) = make_type_param(&interner, "T");
    let (u_name, u_type) = make_type_param(&interner, "U");
    let _var_t = ctx.fresh_type_param(t_name, false);
    let _var_u = ctx.fresh_type_param(u_name, false);

    let name_x = interner.intern_string("x");
    let name_y = interner.intern_string("y");

    let source = interner.object(vec![PropertyInfo::new(name_x, TypeId::STRING)]);
    let target = interner.object(vec![
        PropertyInfo::new(name_x, t_type),
        PropertyInfo::new(name_y, u_type),
    ]);

    ctx.infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    let var_t = ctx.find_type_param(t_name).unwrap();
    let var_u = ctx.find_type_param(u_name).unwrap();

    assert_eq!(ctx.resolve_with_constraints(var_t).unwrap(), TypeId::STRING);
    assert!(ctx.probe(var_u).is_none());
}

// =============================================================================
// Readonly Type Matching
// =============================================================================

#[test]
fn test_match_readonly_unwrap() {
    // Match `readonly number[]` against `readonly T[]`
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, t_type) = make_type_param(&interner, "T");
    let _var_t = ctx.fresh_type_param(t_name, false);

    let source_inner = interner.array(TypeId::NUMBER);
    let source = interner.intern(TypeData::ReadonlyType(source_inner));

    let target_inner = interner.array(t_type);
    let target = interner.intern(TypeData::ReadonlyType(target_inner));

    ctx.infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    let var_t = ctx.find_type_param(t_name).unwrap();
    assert_eq!(ctx.resolve_with_constraints(var_t).unwrap(), TypeId::NUMBER);
}

#[test]
fn test_match_mutable_source_against_readonly_target() {
    // Match `number[]` against `readonly T[]`
    // mutable source is compatible with readonly target
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, t_type) = make_type_param(&interner, "T");
    let _var_t = ctx.fresh_type_param(t_name, false);

    let source = interner.array(TypeId::NUMBER);
    let target_inner = interner.array(t_type);
    let target = interner.intern(TypeData::ReadonlyType(target_inner));

    ctx.infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    let var_t = ctx.find_type_param(t_name).unwrap();
    assert_eq!(ctx.resolve_with_constraints(var_t).unwrap(), TypeId::NUMBER);
}

// =============================================================================
// NoInfer Matching
// =============================================================================

#[test]
fn test_match_noinfer_blocks_inference() {
    // Match `string` against `NoInfer<T>` - should NOT infer T
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, t_type) = make_type_param(&interner, "T");
    let _var_t = ctx.fresh_type_param(t_name, false);

    let target = interner.intern(TypeData::NoInfer(t_type));

    ctx.infer_from_types(TypeId::STRING, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    // T should remain unresolved due to NoInfer
    let var_t = ctx.find_type_param(t_name).unwrap();
    assert!(ctx.probe(var_t).is_none());
}

// =============================================================================
// Intersection Matching
// =============================================================================

#[test]
fn test_match_intersection_target() {
    // Match `{ x: string, y: number }` against `{ x: T } & { y: U }`
    // Each member of the intersection should be tried
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, t_type) = make_type_param(&interner, "T");
    let (u_name, u_type) = make_type_param(&interner, "U");
    let _var_t = ctx.fresh_type_param(t_name, false);
    let _var_u = ctx.fresh_type_param(u_name, false);

    let name_x = interner.intern_string("x");
    let name_y = interner.intern_string("y");

    let source = interner.object(vec![
        PropertyInfo::new(name_x, TypeId::STRING),
        PropertyInfo::new(name_y, TypeId::NUMBER),
    ]);

    let part1 = interner.object(vec![PropertyInfo::new(name_x, t_type)]);
    let part2 = interner.object(vec![PropertyInfo::new(name_y, u_type)]);
    let target = interner.intersection(vec![part1, part2]);

    ctx.infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    let var_t = ctx.find_type_param(t_name).unwrap();
    let var_u = ctx.find_type_param(u_name).unwrap();

    assert_eq!(ctx.resolve_with_constraints(var_t).unwrap(), TypeId::STRING);
    assert_eq!(ctx.resolve_with_constraints(var_u).unwrap(), TypeId::NUMBER);
}

// =============================================================================
// TypeApplication Matching
// =============================================================================

#[test]
fn test_match_equivalent_application_bases_infers_args() {
    let interner = TypeInterner::new();
    let source_def = DefId(143_510);
    let target_def = DefId(143_511);
    let resolver = CanonicalApplicationResolver {
        left: source_def,
        right: target_def,
        canonical: DefId(143_512),
    };
    let mut ctx = InferenceContext::new(&interner);
    ctx.resolver = Some(&resolver);

    let (payload_name, payload_type) = make_type_param(&interner, "Payload");
    let var_payload = ctx.fresh_type_param(payload_name, false);

    let source = interner.application(interner.lazy(source_def), vec![TypeId::NUMBER]);
    let target = interner.application(interner.lazy(target_def), vec![payload_type]);

    ctx.infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    assert_eq!(
        ctx.resolve_with_constraints(var_payload).unwrap(),
        TypeId::NUMBER
    );
}

#[test]
fn test_union_target_prefers_equivalent_application_arm() {
    let interner = TypeInterner::new();
    let source_def = DefId(143_520);
    let target_def = DefId(143_521);
    let resolver = CanonicalApplicationResolver {
        left: source_def,
        right: target_def,
        canonical: DefId(143_522),
    };
    let mut ctx = InferenceContext::new(&interner);
    ctx.resolver = Some(&resolver);

    let (payload_name, payload_type) = make_type_param(&interner, "Slot");
    let (fallback_name, fallback_type) = make_type_param(&interner, "Fallback");
    let var_payload = ctx.fresh_type_param(payload_name, false);
    let var_fallback = ctx.fresh_type_param(fallback_name, false);

    let source = interner.application(interner.lazy(source_def), vec![TypeId::STRING]);
    let structured_target = interner.application(interner.lazy(target_def), vec![payload_type]);
    let target = interner.union(vec![structured_target, fallback_type]);

    ctx.infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    assert_eq!(
        ctx.resolve_with_constraints(var_payload).unwrap(),
        TypeId::STRING
    );
    assert!(!ctx.var_has_candidates(var_fallback));
}

#[test]
fn test_nested_equivalent_application_bases_infer_through_object_property() {
    let interner = TypeInterner::new();
    let source_def = DefId(143_530);
    let target_def = DefId(143_531);
    let resolver = CanonicalApplicationResolver {
        left: source_def,
        right: target_def,
        canonical: DefId(143_532),
    };
    let mut ctx = InferenceContext::new(&interner);
    ctx.resolver = Some(&resolver);

    let (item_name, item_type) = make_type_param(&interner, "Item");
    let var_item = ctx.fresh_type_param(item_name, false);
    let value_atom = interner.intern_string("value");

    let source_app = interner.application(interner.lazy(source_def), vec![TypeId::BOOLEAN]);
    let target_app = interner.application(interner.lazy(target_def), vec![item_type]);
    let source = interner.object(vec![PropertyInfo::new(value_atom, source_app)]);
    let target = interner.object(vec![PropertyInfo::new(value_atom, target_app)]);

    ctx.infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    assert_eq!(
        ctx.resolve_with_constraints(var_item).unwrap(),
        TypeId::BOOLEAN
    );
}

#[test]
fn test_unrelated_application_bases_do_not_infer_args_by_arity() {
    let interner = TypeInterner::new();
    let resolver = CanonicalApplicationResolver {
        left: DefId(143_540),
        right: DefId(143_541),
        canonical: DefId(143_542),
    };
    let mut ctx = InferenceContext::new(&interner);
    ctx.resolver = Some(&resolver);

    let (payload_name, payload_type) = make_type_param(&interner, "Payload");
    let var_payload = ctx.fresh_type_param(payload_name, false);

    let source = interner.application(interner.lazy(DefId(143_543)), vec![TypeId::NUMBER]);
    let target = interner.application(interner.lazy(DefId(143_544)), vec![payload_type]);

    ctx.infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    assert!(!ctx.var_has_candidates(var_payload));
}

// =============================================================================
// Index Access Matching
// =============================================================================

#[test]
fn test_match_index_access() {
    // Match `IndexAccess(A, B)` against `IndexAccess(T, U)`
    // => T = A, U = B
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, t_type) = make_type_param(&interner, "T");
    let (u_name, u_type) = make_type_param(&interner, "U");
    let _var_t = ctx.fresh_type_param(t_name, false);
    let _var_u = ctx.fresh_type_param(u_name, false);

    let source = interner.intern(TypeData::IndexAccess(TypeId::STRING, TypeId::NUMBER));
    let target = interner.intern(TypeData::IndexAccess(t_type, u_type));

    ctx.infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    let var_t = ctx.find_type_param(t_name).unwrap();
    let var_u = ctx.find_type_param(u_name).unwrap();

    assert_eq!(ctx.resolve_with_constraints(var_t).unwrap(), TypeId::STRING);
    assert_eq!(ctx.resolve_with_constraints(var_u).unwrap(), TypeId::NUMBER);
}

#[test]
fn test_union_target_prefers_deferred_index_access_arm() {
    // Match `IndexAccess(A, B)` against `IndexAccess(T, U) | F`.
    // The deferred indexed-access arm is the structured arm: it must receive
    // pairwise inference before the naked fallback can capture the whole source.
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, t_type) = make_type_param(&interner, "T");
    let (u_name, u_type) = make_type_param(&interner, "U");
    let (fallback_name, fallback_type) = make_type_param(&interner, "Fallback");
    let _var_t = ctx.fresh_type_param(t_name, false);
    let _var_u = ctx.fresh_type_param(u_name, false);
    let _var_fallback = ctx.fresh_type_param(fallback_name, false);

    let source = interner.intern(TypeData::IndexAccess(TypeId::STRING, TypeId::NUMBER));
    let structured_target = interner.intern(TypeData::IndexAccess(t_type, u_type));
    let target = interner.union(vec![structured_target, fallback_type]);

    ctx.infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    let var_t = ctx.find_type_param(t_name).unwrap();
    let var_u = ctx.find_type_param(u_name).unwrap();
    let var_fallback = ctx.find_type_param(fallback_name).unwrap();

    assert_eq!(ctx.resolve_with_constraints(var_t).unwrap(), TypeId::STRING);
    assert_eq!(ctx.resolve_with_constraints(var_u).unwrap(), TypeId::NUMBER);
    assert_eq!(
        ctx.resolve_with_constraints(var_fallback).unwrap(),
        TypeId::UNKNOWN,
        "naked fallback must not swallow the deferred indexed-access source"
    );
}

#[test]
fn test_union_target_prefers_keyof_arm() {
    // Match `keyof A` against `keyof T | F`. The structural `keyof` arm must
    // receive inference before the naked fallback can capture the source.
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, t_type) = make_type_param(&interner, "T");
    let (fallback_name, fallback_type) = make_type_param(&interner, "Fallback");
    let _var_t = ctx.fresh_type_param(t_name, false);
    let _var_fallback = ctx.fresh_type_param(fallback_name, false);

    let source = interner.intern(TypeData::KeyOf(TypeId::STRING));
    let structured_target = interner.intern(TypeData::KeyOf(t_type));
    let target = interner.union(vec![structured_target, fallback_type]);

    ctx.infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    let var_t = ctx.find_type_param(t_name).unwrap();
    let var_fallback = ctx.find_type_param(fallback_name).unwrap();

    assert_eq!(ctx.resolve_with_constraints(var_t).unwrap(), TypeId::STRING);
    assert_eq!(
        ctx.resolve_with_constraints(var_fallback).unwrap(),
        TypeId::UNKNOWN,
        "naked fallback must not swallow the `keyof` source"
    );
}

// =============================================================================
// Upper Bound (Source Position) Matching
// =============================================================================

#[test]
fn test_match_type_param_in_source_adds_upper_bound() {
    // When T appears as source, it becomes an upper bound: T <: target
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, t_type) = make_type_param(&interner, "T");
    let _var_t = ctx.fresh_type_param(t_name, false);

    // Infer T <: string (T is the source)
    ctx.infer_from_types(t_type, TypeId::STRING, InferencePriority::NakedTypeVariable)
        .unwrap();

    let var_t = ctx.find_type_param(t_name).unwrap();
    let constraints = ctx.get_constraints(var_t).unwrap();
    assert!(constraints.upper_bounds.contains(&TypeId::STRING));
}

// =============================================================================
// Multiple Candidates
// =============================================================================

#[test]
fn test_match_multiple_sources_same_param() {
    // Two inferences into T: T = string and T = number
    // tsc unions candidates with same priority: result is string | number
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, t_type) = make_type_param(&interner, "T");
    let _var_t = ctx.fresh_type_param(t_name, false);

    ctx.infer_from_types(TypeId::STRING, t_type, InferencePriority::NakedTypeVariable)
        .unwrap();
    ctx.infer_from_types(TypeId::NUMBER, t_type, InferencePriority::NakedTypeVariable)
        .unwrap();

    let var_t = ctx.find_type_param(t_name).unwrap();
    let result = ctx.resolve_with_constraints(var_t).unwrap();
    // tsc unions multiple candidates at the same priority level
    let expected_union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    assert_eq!(result, expected_union);
}

// =============================================================================
// Callable Matching
// =============================================================================

#[test]
fn test_match_callable_signatures() {
    // Match a callable with call signature against another with T
    let interner = TypeInterner::new();
    let mut ctx = InferenceContext::new(&interner);

    let (t_name, t_type) = make_type_param(&interner, "T");
    let _var_t = ctx.fresh_type_param(t_name, false);

    let source = interner.callable(CallableShape {
        call_signatures: vec![CallSignature {
            type_params: Vec::new(),
            params: vec![ParamInfo {
                suppress_display_optional: false,
                name: Some(interner.intern_string("x")),
                type_id: TypeId::NUMBER,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: TypeId::STRING,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        symbol: None,
        is_abstract: false,
        ..Default::default()
    });

    let target = interner.callable(CallableShape {
        call_signatures: vec![CallSignature {
            type_params: Vec::new(),
            params: vec![ParamInfo {
                suppress_display_optional: false,
                name: Some(interner.intern_string("x")),
                type_id: TypeId::NUMBER,
                optional: false,
                rest: false,
            }],
            this_type: None,
            return_type: t_type,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        symbol: None,
        is_abstract: false,
        ..Default::default()
    });

    ctx.infer_from_types(source, target, InferencePriority::NakedTypeVariable)
        .unwrap();

    let var_t = ctx.find_type_param(t_name).unwrap();
    assert_eq!(ctx.resolve_with_constraints(var_t).unwrap(), TypeId::STRING);
}
