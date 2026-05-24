//! Infer-pattern callable arity tests (issue #9662 and follow-ups).
//!
//! Covers conditional `extends` against a callable inference pattern where the
//! source callable supplies fewer parameters than the pattern: the unmatched
//! `infer` slots must default to `unknown` (true branch), including when the
//! infer is nested inside object properties or deferred/generic shells.
//!
//! Split out of `conditional_comprehensive_tests.rs` to keep both files under
//! the repo file-size ceiling and to stop new tests piling onto that file's
//! tail (a recurring merge-conflict point).

use super::*;
use crate::evaluation::evaluate::evaluate_type;
use crate::intern::TypeInterner;
use crate::types::{ConditionalType, TypeData, TypeParamInfo};

// =============================================================================
// Infer-pattern callable arity: source with fewer parameters (issue #9662)
// =============================================================================
//
// Rule: when a source callable is matched against an inference pattern
// `(arg: infer A) => any` inside a conditional `extends`, a source with fewer
// parameters is still assignable; the unmatched `infer` positions default to
// `unknown` and the conditional takes the true branch. These previously took
// the false branch because the params matcher rejected shorter sources.

/// Build a one-parameter infer pattern `(p: infer <name>) => any`.
fn one_arg_infer_pattern(interner: &TypeInterner, name: &str) -> (TypeId, TypeId) {
    let infer_ty = interner.intern(TypeData::Infer(TypeParamInfo {
        name: interner.intern_string(name),
        constraint: None,
        default: None,
        is_const: false,
    }));
    let pattern = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("p")),
            type_id: infer_ty,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::ANY,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    (pattern, infer_ty)
}

#[test]
fn infer_callable_fewer_params_defaults_unmatched_to_unknown() {
    // `(() => void) extends (p: infer A) => any ? A : "no"` → unknown.
    let interner = TypeInterner::new();
    let (pattern, infer_a) = one_arg_infer_pattern(&interner, "A");
    let source = interner.function(FunctionShape::new(vec![], TypeId::VOID));
    let cond = ConditionalType {
        check_type: source,
        extends_type: pattern,
        true_type: infer_a,
        false_type: interner.literal_string("no"),
        is_distributive: false,
    };
    let cond_id = interner.conditional(cond);
    assert_eq!(
        evaluate_type(&interner, cond_id),
        TypeId::UNKNOWN,
        "a source function with fewer params is assignable; the unmatched infer \
         slot must default to unknown (true branch)"
    );
}

#[test]
fn infer_callable_fewer_params_renamed_var_is_structural() {
    // Same as above with the infer variable named `Q` — must behave identically.
    let interner = TypeInterner::new();
    let (pattern, infer_q) = one_arg_infer_pattern(&interner, "Q");
    let source = interner.function(FunctionShape::new(vec![], TypeId::VOID));
    let cond = ConditionalType {
        check_type: source,
        extends_type: pattern,
        true_type: infer_q,
        false_type: interner.literal_string("no"),
        is_distributive: false,
    };
    let cond_id = interner.conditional(cond);
    assert_eq!(
        evaluate_type(&interner, cond_id),
        TypeId::UNKNOWN,
        "infer-variable name must not affect the result"
    );
}

#[test]
fn infer_callable_two_missing_params_default_each_to_unknown() {
    // `(() => void) extends (a: infer A, b: infer B) => any ? [A, B] : "no"`
    // → [unknown, unknown].
    let interner = TypeInterner::new();
    let infer_a = interner.intern(TypeData::Infer(TypeParamInfo {
        name: interner.intern_string("A"),
        constraint: None,
        default: None,
        is_const: false,
    }));
    let infer_b = interner.intern(TypeData::Infer(TypeParamInfo {
        name: interner.intern_string("B"),
        constraint: None,
        default: None,
        is_const: false,
    }));
    let pattern = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![
            ParamInfo {
                name: Some(interner.intern_string("a")),
                type_id: infer_a,
                optional: false,
                rest: false,
            },
            ParamInfo {
                name: Some(interner.intern_string("b")),
                type_id: infer_b,
                optional: false,
                rest: false,
            },
        ],
        this_type: None,
        return_type: TypeId::ANY,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let true_tuple = interner.tuple(vec![
        crate::types::TupleElement {
            type_id: infer_a,
            name: None,
            optional: false,
            rest: false,
        },
        crate::types::TupleElement {
            type_id: infer_b,
            name: None,
            optional: false,
            rest: false,
        },
    ]);
    let source = interner.function(FunctionShape::new(vec![], TypeId::VOID));
    let cond = ConditionalType {
        check_type: source,
        extends_type: pattern,
        true_type: true_tuple,
        false_type: interner.literal_string("no"),
        is_distributive: false,
    };
    let cond_id = interner.conditional(cond);
    let expected = interner.tuple(vec![
        crate::types::TupleElement {
            type_id: TypeId::UNKNOWN,
            name: None,
            optional: false,
            rest: false,
        },
        crate::types::TupleElement {
            type_id: TypeId::UNKNOWN,
            name: None,
            optional: false,
            rest: false,
        },
    ]);
    assert_eq!(
        evaluate_type(&interner, cond_id),
        expected,
        "both unmatched infer slots default to unknown → [unknown, unknown]"
    );
}

#[test]
fn infer_callable_supplied_param_still_infers_concrete_type() {
    // Control: `((x: number) => void) extends (p: infer A) => any ? A : "no"`
    // → number. A supplied parameter must win over the unknown default.
    let interner = TypeInterner::new();
    let (pattern, infer_a) = one_arg_infer_pattern(&interner, "A");
    let source = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::NUMBER,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let cond = ConditionalType {
        check_type: source,
        extends_type: pattern,
        true_type: infer_a,
        false_type: interner.literal_string("no"),
        is_distributive: false,
    };
    let cond_id = interner.conditional(cond);
    assert_eq!(
        evaluate_type(&interner, cond_id),
        TypeId::NUMBER,
        "a matched parameter must infer its concrete type, not the unknown default"
    );
}

#[test]
fn infer_callable_non_function_source_takes_false_branch() {
    // Negative: `string extends (p: infer A) => any ? A : "no"` → "no".
    let interner = TypeInterner::new();
    let (pattern, infer_a) = one_arg_infer_pattern(&interner, "A");
    let no_lit = interner.literal_string("no");
    let cond = ConditionalType {
        check_type: TypeId::STRING,
        extends_type: pattern,
        true_type: infer_a,
        false_type: no_lit,
        is_distributive: false,
    };
    let cond_id = interner.conditional(cond);
    assert_eq!(
        evaluate_type(&interner, cond_id),
        no_lit,
        "a non-function source must still take the false branch"
    );
}

#[test]
fn infer_callable_unmatched_param_defaults_nested_object_infer() {
    // `(() => void) extends (x: { value: infer A }) => any ? A : never` → unknown.
    // The unmatched parameter's infer is nested inside an object property, so a
    // top-level-only collector would miss it and leak an unresolved `infer A`
    // into the true branch. It must still default to `unknown`.
    let interner = TypeInterner::new();
    let infer_a = interner.intern(TypeData::Infer(TypeParamInfo {
        name: interner.intern_string("A"),
        constraint: None,
        default: None,
        is_const: false,
    }));
    let param_obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("value"),
        infer_a,
    )]);
    let pattern = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: param_obj,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::ANY,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let source = interner.function(FunctionShape::new(vec![], TypeId::VOID));
    let cond = ConditionalType {
        check_type: source,
        extends_type: pattern,
        true_type: infer_a,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };
    let cond_id = interner.conditional(cond);
    assert_eq!(
        evaluate_type(&interner, cond_id),
        TypeId::UNKNOWN,
        "an unmatched parameter whose infer is nested in an object property must \
         still default to unknown, not leak an unresolved infer"
    );
}

#[test]
fn infer_callable_unmatched_param_defaults_deferred_application_infer() {
    // `(() => void) extends (x: Box<infer A>) => any ? A : never` → unknown.
    // The unmatched parameter's infer is inside a generic application shell
    // (a deferred shape). The default-fill walk must descend into application
    // arguments too, otherwise `A` leaks unresolved into the true branch.
    let interner = TypeInterner::new();
    let infer_a = interner.intern(TypeData::Infer(TypeParamInfo {
        name: interner.intern_string("A"),
        constraint: None,
        default: None,
        is_const: false,
    }));
    // Base `Box` is an ordinary (non-infer) shell; the infer lives only in the
    // application argument, so this exercises the Application-arg traversal.
    let box_base = interner.object(vec![PropertyInfo::new(
        interner.intern_string("inner"),
        TypeId::UNKNOWN,
    )]);
    let box_of_infer = interner.application(box_base, vec![infer_a]);
    let pattern = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: box_of_infer,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::ANY,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });
    let source = interner.function(FunctionShape::new(vec![], TypeId::VOID));
    let cond = ConditionalType {
        check_type: source,
        extends_type: pattern,
        true_type: infer_a,
        false_type: TypeId::NEVER,
        is_distributive: false,
    };
    let cond_id = interner.conditional(cond);
    assert_eq!(
        evaluate_type(&interner, cond_id),
        TypeId::UNKNOWN,
        "an unmatched parameter whose infer is nested in a generic application \
         shell must still default to unknown"
    );
}
