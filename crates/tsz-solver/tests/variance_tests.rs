//! Comprehensive tests for variance calculation.
//!
//! These tests verify the variance analysis for generic type parameters:
//! - Covariant positions (array elements, function returns, readonly properties)
//! - Contravariant positions (function parameters)
//! - Invariant positions (mutable properties with different read/write types)
//! - Bivariant positions (method parameters in TypeScript)
//! - Independent variance (type parameter unused)
//! - Variance composition through nested generics
//! - Variance through conditional, mapped, union, intersection types

use super::*;
use crate::intern::TypeInterner;
use crate::relations::variance::compute_variance;
use crate::types::{
    ConditionalType, FunctionShape, MappedModifier, MappedType, ObjectFlags, ObjectShape,
    ParamInfo, PropertyInfo, TupleElement, TypeParamInfo, Variance,
};

fn create_interner() -> TypeInterner {
    TypeInterner::new()
}

/// Helper to create a `TypeParamInfo` with just a name.
fn type_param(interner: &TypeInterner, name: &str) -> TypeParamInfo {
    TypeParamInfo {
        name: interner.intern_string(name),
        constraint: None,
        default: None,
        is_const: false,
    }
}

/// Helper to create and intern a type parameter type.
fn intern_type_param(interner: &TypeInterner, name: &str) -> TypeId {
    interner.type_param(type_param(interner, name))
}

// =============================================================================
// Covariant Position Tests
// =============================================================================

#[test]
fn test_variance_covariant_array_element() {
    // Array<T> - T is in covariant position (element type)
    let interner = create_interner();
    let t_param = intern_type_param(&interner, "T");
    let array_of_t = interner.array(t_param);

    let t_atom = interner.intern_string("T");
    let variance = compute_variance(&interner, array_of_t, t_atom);

    assert!(variance.is_covariant(), "Array element should be covariant");
    assert!(
        !variance.is_contravariant(),
        "Array element should not be contravariant"
    );
    assert!(
        !variance.is_invariant(),
        "Array element should not be invariant"
    );
}

#[test]
fn test_variance_covariant_function_return() {
    // () => T - T is in covariant position (return type)
    let interner = create_interner();
    let t_param = intern_type_param(&interner, "T");

    let func = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: t_param,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let t_atom = interner.intern_string("T");
    let variance = compute_variance(&interner, func, t_atom);

    assert!(
        variance.is_covariant(),
        "Function return type should be covariant"
    );
    assert!(
        !variance.is_contravariant(),
        "Function return should not be contravariant"
    );
}

#[test]
fn test_variance_covariant_readonly_property() {
    // { readonly x: T } - T is in covariant position
    let interner = create_interner();
    let t_param = intern_type_param(&interner, "T");
    let x_atom = interner.intern_string("x");

    let obj = interner.object(vec![PropertyInfo::readonly(x_atom, t_param)]);

    let t_atom = interner.intern_string("T");
    let variance = compute_variance(&interner, obj, t_atom);

    assert!(
        variance.is_covariant(),
        "Readonly property should be covariant"
    );
}

#[test]
fn test_variance_covariant_tuple_element() {
    // [T, number] - T is in covariant position
    let interner = create_interner();
    let t_param = intern_type_param(&interner, "T");

    let tuple = interner.tuple(vec![
        TupleElement {
            type_id: t_param,
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

    let t_atom = interner.intern_string("T");
    let variance = compute_variance(&interner, tuple, t_atom);

    assert!(variance.is_covariant(), "Tuple element should be covariant");
}

// =============================================================================
// Contravariant Position Tests
// =============================================================================

#[test]
fn test_variance_contravariant_function_parameter() {
    // (x: T) => void - T is in contravariant position (parameter)
    let interner = create_interner();
    let t_param = intern_type_param(&interner, "T");

    let func = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: t_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let t_atom = interner.intern_string("T");
    let variance = compute_variance(&interner, func, t_atom);

    assert!(
        variance.is_contravariant(),
        "Function parameter should be contravariant"
    );
    assert!(
        !variance.is_covariant(),
        "Function parameter should not be covariant"
    );
}

#[test]
fn test_variance_contravariant_callback_parameter() {
    // (cb: (x: T) => void) => void
    // The T in the callback parameter is doubly flipped:
    // - callback param is contravariant (flip 1)
    // - T in callback's param is contravariant (flip 2)
    // Double flip = covariant
    let interner = create_interner();
    let t_param = intern_type_param(&interner, "T");

    let callback = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: t_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let outer = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("cb")),
            type_id: callback,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let t_atom = interner.intern_string("T");
    let variance = compute_variance(&interner, outer, t_atom);

    assert!(
        variance.is_covariant(),
        "Doubly nested contravariance should be covariant"
    );
}

#[test]
fn test_variance_contravariant_keyof() {
    // keyof T - reverses variance
    let interner = create_interner();
    let t_param = intern_type_param(&interner, "T");
    let keyof_t = interner.keyof(t_param);

    let t_atom = interner.intern_string("T");
    let variance = compute_variance(&interner, keyof_t, t_atom);

    assert!(
        variance.is_contravariant(),
        "keyof should make T contravariant"
    );
}

// =============================================================================
// Invariant Position Tests
// =============================================================================

#[test]
fn test_variance_invariant_both_positions() {
    // { get(): T; set(x: T): void } - T is in both covariant and contravariant positions
    let interner = create_interner();
    let t_param = intern_type_param(&interner, "T");

    let getter = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: t_param,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let setter = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: t_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let get_atom = interner.intern_string("get");
    let set_atom = interner.intern_string("set");

    let obj = interner.object(vec![
        PropertyInfo::method(get_atom, getter),
        PropertyInfo::method(set_atom, setter),
    ]);

    let t_atom = interner.intern_string("T");
    let variance = compute_variance(&interner, obj, t_atom);

    assert!(
        variance.is_invariant(),
        "Type in both get/set positions should be invariant"
    );
}

#[test]
fn test_variance_invariant_explicit_write_type() {
    // Property with different write_type triggers contravariant visit
    let interner = create_interner();
    let t_param = intern_type_param(&interner, "T");
    let x_atom = interner.intern_string("x");

    // Create an object where read_type=T and write_type=T and they differ (different from read)
    // This simulates a set accessor with a different type
    let u_param = intern_type_param(&interner, "U");
    let obj = interner.object(vec![PropertyInfo {
        name: x_atom,
        type_id: t_param,    // read type: T (covariant)
        write_type: u_param, // write type: U (different, so triggers contravariant visit)
        optional: false,
        readonly: false,
        is_method: false,
        is_class_prototype: false,
        visibility: Visibility::Public,
        parent_id: None,
        declaration_order: 0,
        is_string_named: false,
    }]);

    let t_atom = interner.intern_string("T");
    let variance = compute_variance(&interner, obj, t_atom);

    // T appears only in read (covariant) position
    assert!(
        variance.is_covariant(),
        "T should be covariant when only in read position"
    );

    let u_atom = interner.intern_string("U");
    let u_variance = compute_variance(&interner, obj, u_atom);

    // U appears only in write (contravariant) position
    assert!(
        u_variance.is_contravariant(),
        "U should be contravariant when only in write position"
    );
}

// =============================================================================
// Bivariant Position Tests (TypeScript method parameters)
// =============================================================================

#[test]
fn test_variance_method_parameters_contravariant() {
    // Method parameters contribute to variance just like regular function parameters.
    // While method bivariance is a TypeScript assignability-level concept (methods
    // have bivariant parameter checking), variance COMPUTATION must still traverse
    // method parameters to discover type parameter positions. Without this,
    // type parameters appearing only in method parameter positions would be
    // incorrectly marked as INDEPENDENT, causing the variance fast path to skip
    // checks entirely for types like Promise<T>.
    let interner = create_interner();
    let t_param = intern_type_param(&interner, "T");

    let method = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: t_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: true, // This is a method!
    });

    let t_atom = interner.intern_string("T");
    let variance = compute_variance(&interner, method, t_atom);

    // Method parameters contribute COVARIANT due to method bivariance.
    // In tsc, method bivariance makes type params appear BIVARIANT through
    // marker types, but checks bivariant using covariant direction first.
    // We match this by recording all method-param occurrences as COVARIANT.
    assert!(
        variance.is_covariant(),
        "Method parameter T should be covariant (method bivariance → covariant-first)"
    );
}

#[test]
fn test_variance_method_return_still_covariant() {
    // Even for methods, return type should still be covariant
    // { m(): T } - T is covariant in return
    let interner = create_interner();
    let t_param = intern_type_param(&interner, "T");

    let method = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: t_param,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });

    let t_atom = interner.intern_string("T");
    let variance = compute_variance(&interner, method, t_atom);

    assert!(
        variance.is_covariant(),
        "Method return type should still be covariant"
    );
}

#[test]
fn test_variance_method_with_callback_param_is_covariant() {
    // Promise<T> pattern: { then<U>(cb: (x: T) => R): R }
    // T appears in callback parameter (contra) of method parameter (contra)
    // double-contravariant = covariant
    let interner = create_interner();
    let t_param = intern_type_param(&interner, "T");

    // Inner callback: (x: T) => void
    let callback = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: t_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    // Outer method: then(cb: (x: T) => void): void
    let method = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("cb")),
            type_id: callback,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: true, // This is a method
    });

    let t_atom = interner.intern_string("T");
    let variance = compute_variance(&interner, method, t_atom);

    // T at double-contravariant depth (method param → callback param) is covariant.
    // This is the key fix: previously method params were skipped entirely, making T
    // independent and causing Promise<Foo> <: Promise<Bar> to skip variance checks.
    assert!(
        variance.is_covariant(),
        "T in callback param of method param should be covariant (contra × contra = co)"
    );
}

// =============================================================================
// Independent Variance Tests
// =============================================================================

#[test]
fn test_variance_independent_unused_param() {
    // type Phantom<T> = number - T is not used at all
    let interner = create_interner();

    // Just TypeId::NUMBER - no reference to T
    let t_atom = interner.intern_string("T");
    let variance = compute_variance(&interner, TypeId::NUMBER, t_atom);

    assert!(
        variance.is_independent(),
        "Unused type parameter should be independent"
    );
    assert!(
        !variance.is_covariant(),
        "Independent should not be covariant"
    );
    assert!(
        !variance.is_contravariant(),
        "Independent should not be contravariant"
    );
}

#[test]
fn test_variance_independent_different_param_name() {
    // Array<U> with target_param = "T" - T is not used
    let interner = create_interner();
    let u_param = intern_type_param(&interner, "U");
    let array_of_u = interner.array(u_param);

    let t_atom = interner.intern_string("T");
    let variance = compute_variance(&interner, array_of_u, t_atom);

    assert!(
        variance.is_independent(),
        "Different param name should be independent"
    );
}

// =============================================================================
// Union and Intersection Variance Tests
// =============================================================================

#[test]
fn test_variance_union_covariant() {
    // T | string - T is covariant (polarity preserved in union)
    let interner = create_interner();
    let t_param = intern_type_param(&interner, "T");
    let union = interner.union(vec![t_param, TypeId::STRING]);

    let t_atom = interner.intern_string("T");
    let variance = compute_variance(&interner, union, t_atom);

    assert!(variance.is_covariant(), "Union member should be covariant");
}

#[test]
fn test_variance_intersection_covariant() {
    // T & { x: number } - T is covariant (polarity preserved in intersection)
    let interner = create_interner();
    let t_param = intern_type_param(&interner, "T");
    let x_atom = interner.intern_string("x");
    let obj = interner.object(vec![PropertyInfo::new(x_atom, TypeId::NUMBER)]);
    let intersection = interner.intersection(vec![t_param, obj]);

    let t_atom = interner.intern_string("T");
    let variance = compute_variance(&interner, intersection, t_atom);

    assert!(
        variance.is_covariant(),
        "Intersection member should be covariant"
    );
}

// =============================================================================
// Nested Variance Composition Tests
// =============================================================================

#[test]
fn test_variance_nested_array_of_arrays() {
    // Array<Array<T>> - T should still be covariant (covariant of covariant = covariant)
    let interner = create_interner();
    let t_param = intern_type_param(&interner, "T");
    let inner_array = interner.array(t_param);
    let outer_array = interner.array(inner_array);

    let t_atom = interner.intern_string("T");
    let variance = compute_variance(&interner, outer_array, t_atom);

    assert!(
        variance.is_covariant(),
        "Nested arrays should preserve covariance"
    );
}

#[test]
fn test_variance_function_returning_function() {
    // () => (x: T) => void
    // Return type is covariant, parameter of inner function is contravariant
    // Covariant(Contravariant) = Contravariant
    let interner = create_interner();
    let t_param = intern_type_param(&interner, "T");

    let inner = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: t_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let outer = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: inner,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let t_atom = interner.intern_string("T");
    let variance = compute_variance(&interner, outer, t_atom);

    assert!(
        variance.is_contravariant(),
        "Covariant(Contravariant) should be contravariant"
    );
}

#[test]
fn test_variance_param_of_param() {
    // (f: (x: T) => void) => void
    // Contravariant(Contravariant(T)) = Covariant
    let interner = create_interner();
    let t_param = intern_type_param(&interner, "T");

    let inner = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: t_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let outer = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("f")),
            type_id: inner,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let t_atom = interner.intern_string("T");
    let variance = compute_variance(&interner, outer, t_atom);

    assert!(
        variance.is_covariant(),
        "Contravariant(Contravariant) should be covariant"
    );
}

// =============================================================================
// Conditional Type Variance Tests
// =============================================================================

#[test]
fn test_variance_conditional_branches() {
    // T extends string ? T : number
    // In tsc, only the branch types contribute to variance (true/false branches)
    // check_type and extends_type are not variance contributors
    let interner = create_interner();
    let t_param = intern_type_param(&interner, "T");

    let cond = interner.conditional(ConditionalType {
        check_type: t_param,
        extends_type: TypeId::STRING,
        true_type: t_param,
        false_type: TypeId::NUMBER,
        is_distributive: false,
    });

    let t_atom = interner.intern_string("T");
    let variance = compute_variance(&interner, cond, t_atom);

    // T appears in true_type at covariant position
    assert!(
        variance.contains(Variance::COVARIANT),
        "T in true branch should be covariant"
    );
}

#[test]
fn test_variance_conditional_false_branch_only() {
    // U extends string ? number : U
    // U appears only in false_type branch (covariant)
    let interner = create_interner();
    let u_param = intern_type_param(&interner, "U");

    let cond = interner.conditional(ConditionalType {
        check_type: u_param,
        extends_type: TypeId::STRING,
        true_type: TypeId::NUMBER,
        false_type: u_param,
        is_distributive: false,
    });

    let u_atom = interner.intern_string("U");
    let variance = compute_variance(&interner, cond, u_atom);

    assert!(
        variance.contains(Variance::COVARIANT),
        "U in false branch should be covariant"
    );
}

// =============================================================================
// Object Shape Variance Tests
// =============================================================================

#[test]
fn test_variance_object_property_covariant() {
    // { x: T } - mutable property
    // In TypeScript, all properties are treated as covariant for variance inference
    // (known unsoundness for usability - matches tsc)
    let interner = create_interner();
    let t_param = intern_type_param(&interner, "T");
    let x_atom = interner.intern_string("x");

    let obj = interner.object(vec![PropertyInfo::new(x_atom, t_param)]);

    let t_atom = interner.intern_string("T");
    let variance = compute_variance(&interner, obj, t_atom);

    // TypeScript treats mutable properties as covariant (unsound but matches tsc)
    assert!(
        variance.is_covariant(),
        "Mutable property should be covariant (TS unsoundness)"
    );
}

#[test]
fn test_variance_object_multiple_properties() {
    // { x: T; y: T } - T appears in two covariant positions
    let interner = create_interner();
    let t_param = intern_type_param(&interner, "T");
    let x_atom = interner.intern_string("x");
    let y_atom = interner.intern_string("y");

    let obj = interner.object(vec![
        PropertyInfo::new(x_atom, t_param),
        PropertyInfo::new(y_atom, t_param),
    ]);

    let t_atom = interner.intern_string("T");
    let variance = compute_variance(&interner, obj, t_atom);

    assert!(
        variance.is_covariant(),
        "Multiple covariant positions should still be covariant"
    );
}

#[test]
fn test_variance_object_index_signature() {
    // { [key: string]: T } - index signature value type
    let interner = create_interner();
    let t_param = intern_type_param(&interner, "T");

    let obj = interner.object_with_index(ObjectShape {
        symbol: None,
        flags: ObjectFlags::empty(),
        properties: Vec::new(),
        string_index: Some(crate::types::IndexSignature {
            key_type: TypeId::STRING,
            value_type: t_param,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
    });

    let t_atom = interner.intern_string("T");
    let variance = compute_variance(&interner, obj, t_atom);

    // Index signature value types are covariant (same tsc parity rule)
    assert!(
        variance.is_covariant(),
        "Index signature value should be covariant"
    );
}

// =============================================================================
// Mapped Type Variance Tests
// =============================================================================

#[test]
fn test_variance_mapped_template_covariant() {
    // { [K in keyof X]: T } - T in template position is covariant
    let interner = create_interner();
    let t_param = intern_type_param(&interner, "T");

    let k_info = type_param(&interner, "K");
    let mapped = interner.mapped(MappedType {
        type_param: k_info,
        constraint: TypeId::STRING,
        template: t_param,
        optional_modifier: None,
        readonly_modifier: None,
        name_type: None,
    });

    let t_atom = interner.intern_string("T");
    let variance = compute_variance(&interner, mapped, t_atom);

    assert!(
        variance.contains(Variance::COVARIANT),
        "Mapped type template should be covariant for T"
    );
}

#[test]
fn test_variance_mapped_with_modifier_needs_structural_fallback() {
    // Required<T> = { [K in keyof T]-?: T[K] }
    // When mapped type has modifiers, variance shortcut is unreliable
    let interner = create_interner();
    let t_param = intern_type_param(&interner, "T");

    let k_info = type_param(&interner, "K");
    let mapped = interner.mapped(MappedType {
        type_param: k_info,
        constraint: t_param,
        template: TypeId::STRING,
        optional_modifier: Some(MappedModifier::Remove), // -? modifier
        readonly_modifier: None,
        name_type: None,
    });

    let t_atom = interner.intern_string("T");
    let variance = compute_variance(&interner, mapped, t_atom);

    assert!(
        variance.needs_structural_fallback(),
        "Mapped type with optional modifier should need structural fallback"
    );
}

// =============================================================================
// Template Literal Variance Tests
// =============================================================================

#[test]
fn test_variance_template_literal_covariant() {
    // `prefix-${T}` - T is covariant in template literal spans
    let interner = create_interner();
    let t_param = intern_type_param(&interner, "T");

    let template = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("prefix-")),
        TemplateSpan::Type(t_param),
    ]);

    let t_atom = interner.intern_string("T");
    let variance = compute_variance(&interner, template, t_atom);

    assert!(
        variance.is_covariant(),
        "Template literal type span should be covariant"
    );
}

// =============================================================================
// Intrinsic / Literal Type Variance Tests
// =============================================================================

#[test]
fn test_variance_intrinsic_no_param() {
    // number, string, etc. - no type parameters
    let interner = create_interner();
    let t_atom = interner.intern_string("T");

    assert!(compute_variance(&interner, TypeId::NUMBER, t_atom).is_independent());
    assert!(compute_variance(&interner, TypeId::STRING, t_atom).is_independent());
    assert!(compute_variance(&interner, TypeId::BOOLEAN, t_atom).is_independent());
    assert!(compute_variance(&interner, TypeId::VOID, t_atom).is_independent());
    assert!(compute_variance(&interner, TypeId::NEVER, t_atom).is_independent());
    assert!(compute_variance(&interner, TypeId::ANY, t_atom).is_independent());
    assert!(compute_variance(&interner, TypeId::UNKNOWN, t_atom).is_independent());
}

#[test]
fn test_variance_literal_no_param() {
    let interner = create_interner();
    let t_atom = interner.intern_string("T");

    let str_lit = interner.literal_string("hello");
    let num_lit = interner.literal_number(42.0);

    assert!(compute_variance(&interner, str_lit, t_atom).is_independent());
    assert!(compute_variance(&interner, num_lit, t_atom).is_independent());
}

// =============================================================================
// Callable Type Variance Tests
// =============================================================================

#[test]
fn test_variance_callable_call_signatures() {
    // Callable with call signature: { (x: T): string }
    let interner = create_interner();
    let t_param = intern_type_param(&interner, "T");

    let callable = interner.callable(CallableShape {
        call_signatures: vec![CallSignature {
            type_params: Vec::new(),
            params: vec![ParamInfo {
                name: Some(interner.intern_string("x")),
                type_id: t_param,
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
        string_index: None,
        number_index: None,
        symbol: None,
        is_abstract: false,
    });

    let t_atom = interner.intern_string("T");
    let variance = compute_variance(&interner, callable, t_atom);

    assert!(
        variance.is_contravariant(),
        "Callable call signature parameter should be contravariant"
    );
}

#[test]
fn test_variance_callable_return_type() {
    // Callable with return type: { (): T }
    let interner = create_interner();
    let t_param = intern_type_param(&interner, "T");

    let callable = interner.callable(CallableShape {
        call_signatures: vec![CallSignature {
            type_params: Vec::new(),
            params: Vec::new(),
            this_type: None,
            return_type: t_param,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: Vec::new(),
        string_index: None,
        number_index: None,
        symbol: None,
        is_abstract: false,
    });

    let t_atom = interner.intern_string("T");
    let variance = compute_variance(&interner, callable, t_atom);

    assert!(
        variance.is_covariant(),
        "Callable return type should be covariant"
    );
}

#[test]
fn test_variance_callable_with_properties() {
    // Callable with property: { (): void; prop: T }
    let interner = create_interner();
    let t_param = intern_type_param(&interner, "T");
    let prop_atom = interner.intern_string("prop");

    let callable = interner.callable(CallableShape {
        call_signatures: vec![CallSignature {
            type_params: Vec::new(),
            params: Vec::new(),
            this_type: None,
            return_type: TypeId::VOID,
            type_predicate: None,
            is_method: false,
        }],
        construct_signatures: Vec::new(),
        properties: vec![PropertyInfo::readonly(prop_atom, t_param)],
        string_index: None,
        number_index: None,
        symbol: None,
        is_abstract: false,
    });

    let t_atom = interner.intern_string("T");
    let variance = compute_variance(&interner, callable, t_atom);

    assert!(
        variance.is_covariant(),
        "Callable readonly property should be covariant"
    );
}

// =============================================================================
// Readonly Type Variance Tests
// =============================================================================

#[test]
fn test_variance_readonly_preserves_polarity() {
    // Readonly<T> - inner type preserves current polarity
    let interner = create_interner();
    let t_param = intern_type_param(&interner, "T");
    let readonly_t = interner.readonly_type(t_param);

    let t_atom = interner.intern_string("T");
    let variance = compute_variance(&interner, readonly_t, t_atom);

    assert!(
        variance.is_covariant(),
        "Readonly should preserve covariant polarity"
    );
}

// =============================================================================
// Index Access Variance Tests
// =============================================================================

#[test]
fn test_variance_index_access() {
    // T[K] - both T and K are at current polarity
    let interner = create_interner();
    let t_param = intern_type_param(&interner, "T");
    let k_param = intern_type_param(&interner, "K");
    let idx = interner.index_access(t_param, k_param);

    let t_atom = interner.intern_string("T");
    let variance = compute_variance(&interner, idx, t_atom);

    assert!(
        variance.is_covariant(),
        "Index access object should be covariant"
    );

    let k_atom = interner.intern_string("K");
    let variance_k = compute_variance(&interner, idx, k_atom);

    assert!(
        variance_k.is_covariant(),
        "Index access key should be covariant"
    );
}

// =============================================================================
// Multiple Position Tests (Invariance from combined usage)
// =============================================================================

#[test]
fn test_variance_param_and_return() {
    // (x: T) => T - T is both contravariant (param) and covariant (return) = invariant
    let interner = create_interner();
    let t_param = intern_type_param(&interner, "T");

    let func = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: t_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_param,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let t_atom = interner.intern_string("T");
    let variance = compute_variance(&interner, func, t_atom);

    assert!(
        variance.is_invariant(),
        "T in both param and return should be invariant"
    );
}

#[test]
fn test_variance_union_of_co_and_contra() {
    // Union: ((x: T) => void) | (() => T)
    // T in first member: contravariant; T in second member: covariant
    // Union = invariant
    let interner = create_interner();
    let t_param = intern_type_param(&interner, "T");

    let consumer = interner.function(FunctionShape {
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: t_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let producer = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: None,
        return_type: t_param,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let union = interner.union(vec![consumer, producer]);

    let t_atom = interner.intern_string("T");
    let variance = compute_variance(&interner, union, t_atom);

    assert!(
        variance.is_invariant(),
        "Union of covariant and contravariant should be invariant"
    );
}

// =============================================================================
// Variance Composition Rules
// =============================================================================

#[test]
fn test_variance_empty_result() {
    // Variance::empty() should be independent
    let v = Variance::empty();
    assert!(v.is_independent());
    assert!(!v.is_covariant());
    assert!(!v.is_contravariant());
    assert!(!v.is_invariant());
}

#[test]
fn test_variance_flag_composition() {
    // Test the Variance bitflags
    let co = Variance::COVARIANT;
    assert!(co.is_covariant());
    assert!(!co.is_contravariant());
    assert!(!co.is_invariant());

    let contra = Variance::CONTRAVARIANT;
    assert!(!contra.is_covariant());
    assert!(contra.is_contravariant());
    assert!(!contra.is_invariant());

    let invariant = Variance::COVARIANT | Variance::CONTRAVARIANT;
    assert!(!invariant.is_covariant()); // is_covariant checks ONLY covariant
    assert!(!invariant.is_contravariant()); // is_contravariant checks ONLY contravariant
    assert!(invariant.is_invariant());
}

// =============================================================================
// This Type Variance Tests
// =============================================================================

#[test]
fn test_variance_this_parameter_function() {
    // function with this parameter: (this: T) => void
    // For non-method functions, this is contravariant
    let interner = create_interner();
    let t_param = intern_type_param(&interner, "T");

    let func = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: Some(t_param),
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let t_atom = interner.intern_string("T");
    let variance = compute_variance(&interner, func, t_atom);

    assert!(
        variance.is_contravariant(),
        "this parameter in non-method function should be contravariant"
    );
}

#[test]
fn test_variance_this_parameter_method() {
    // For methods, this parameter keeps current polarity (covariant)
    let interner = create_interner();
    let t_param = intern_type_param(&interner, "T");

    let method = interner.function(FunctionShape {
        params: Vec::new(),
        this_type: Some(t_param),
        return_type: TypeId::VOID,
        type_params: Vec::new(),
        type_predicate: None,
        is_constructor: false,
        is_method: true,
    });

    let t_atom = interner.intern_string("T");
    let variance = compute_variance(&interner, method, t_atom);

    assert!(
        variance.is_covariant(),
        "this parameter in method should be covariant"
    );
}

// =============================================================================
// String Intrinsic Variance Tests
// =============================================================================

#[test]
fn test_variance_string_intrinsic() {
    // Uppercase<T> - T is at current polarity
    let interner = create_interner();
    let t_param = intern_type_param(&interner, "T");

    let upper = interner.string_intrinsic(crate::types::StringIntrinsicKind::Uppercase, t_param);

    let t_atom = interner.intern_string("T");
    let variance = compute_variance(&interner, upper, t_atom);

    assert!(
        variance.is_covariant(),
        "String intrinsic type arg should be covariant"
    );
}

// =============================================================================
// Infer Type Variance Tests
// =============================================================================

#[test]
fn test_variance_infer_does_not_contribute() {
    // infer T - declaration, not a usage of target param
    let interner = create_interner();

    let infer_t = interner.infer(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
    });

    let t_atom = interner.intern_string("T");
    let variance = compute_variance(&interner, infer_t, t_atom);

    // infer T declares T, doesn't use it - should be independent
    assert!(
        variance.is_independent(),
        "infer T should be independent (it's a declaration, not usage)"
    );
}

// =============================================================================
// Type Parameter Constraint Variance Tests
// =============================================================================

#[test]
fn test_variance_type_param_with_constraint() {
    // When a type parameter has a constraint that mentions the target,
    // the constraint contributes at the current polarity
    let interner = create_interner();
    let t_atom_val = interner.intern_string("T");
    let t_param_type = interner.type_param(TypeParamInfo {
        name: t_atom_val,
        constraint: None,
        default: None,
        is_const: false,
    });

    // U extends T - the constraint references T at covariant position
    let u_with_constraint = interner.type_param(TypeParamInfo {
        name: interner.intern_string("U"),
        constraint: Some(t_param_type),
        default: None,
        is_const: false,
    });

    let t_atom = interner.intern_string("T");
    let variance = compute_variance(&interner, u_with_constraint, t_atom);

    assert!(
        variance.is_covariant(),
        "Type parameter constraint should contribute at current polarity"
    );
}
