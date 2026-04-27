use super::*;
use crate::TypeDatabase;
use crate::def::{DefId, DefKind};
use crate::intern::TypeInterner;
use crate::relations::subtype::{TypeEnvironment, TypeResolver};
use crate::types::{PropertyInfo, SymbolRef, TypeData, TypeParamInfo};

// ===================================================================
// Helper resolvers for testing
// ===================================================================

#[test]
fn test_canonicalizer_creation() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();
    let _canonicalizer = Canonicalizer::new(&interner, &env);
}

// ===================================================================
// Primitive identity preservation
// ===================================================================

#[test]
fn test_canonicalize_primitive() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();
    let mut canon = Canonicalizer::new(&interner, &env);

    let number = TypeId::NUMBER;
    let canon_number = canon.canonicalize(number);

    // Primitives should canonicalize to themselves
    assert_eq!(canon_number, number);
}

#[test]
fn canonicalize_all_primitives_identity() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();
    let mut canon = Canonicalizer::new(&interner, &env);

    let primitives = [
        TypeId::NEVER,
        TypeId::UNKNOWN,
        TypeId::ANY,
        TypeId::VOID,
        TypeId::UNDEFINED,
        TypeId::NULL,
        TypeId::BOOLEAN,
        TypeId::NUMBER,
        TypeId::STRING,
        TypeId::BIGINT,
        TypeId::SYMBOL,
        TypeId::OBJECT,
        TypeId::FUNCTION,
        TypeId::ERROR,
        TypeId::BOOLEAN_TRUE,
        TypeId::BOOLEAN_FALSE,
    ];

    for prim in primitives {
        let result = canon.canonicalize(prim);
        assert_eq!(
            result, prim,
            "Primitive TypeId({}) should canonicalize to itself",
            prim.0
        );
    }
}

// ===================================================================
// Literal identity preservation
// ===================================================================

#[test]
fn canonicalize_string_literal_identity() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();
    let mut canon = Canonicalizer::new(&interner, &env);

    let lit = interner.literal_string("hello");
    let result = canon.canonicalize(lit);
    assert_eq!(result, lit);
}

#[test]
fn canonicalize_number_literal_identity() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();
    let mut canon = Canonicalizer::new(&interner, &env);

    let lit = interner.literal_number(42.0);
    let result = canon.canonicalize(lit);
    assert_eq!(result, lit);
}

#[test]
fn canonicalize_boolean_literal_identity() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();
    let mut canon = Canonicalizer::new(&interner, &env);

    let t = interner.literal_boolean(true);
    let f = interner.literal_boolean(false);
    assert_eq!(canon.canonicalize(t), t);
    assert_eq!(canon.canonicalize(f), f);
}

// ===================================================================
// Array canonicalization
// ===================================================================

#[test]
fn canonicalize_array_of_primitive() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();
    let mut canon = Canonicalizer::new(&interner, &env);

    let arr = interner.array(TypeId::STRING);
    let result = canon.canonicalize(arr);
    // Array of primitive should be identical (element doesn't change)
    assert_eq!(result, arr);
}

#[test]
fn canonicalize_nested_array() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();
    let mut canon = Canonicalizer::new(&interner, &env);

    // number[][]
    let inner = interner.array(TypeId::NUMBER);
    let outer = interner.array(inner);
    let result = canon.canonicalize(outer);
    assert_eq!(result, outer);
}

#[test]
fn canonicalize_array_structural_equivalence() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();
    let mut canon = Canonicalizer::new(&interner, &env);

    // Two arrays with the same element type should produce the same canonical form
    let arr1 = interner.array(TypeId::NUMBER);
    let arr2 = interner.array(TypeId::NUMBER);
    assert_eq!(canon.canonicalize(arr1), canon.canonicalize(arr2));
}

// ===================================================================
// Tuple canonicalization
// ===================================================================

#[test]
fn canonicalize_tuple_of_primitives() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();
    let mut canon = Canonicalizer::new(&interner, &env);

    use crate::types::TupleElement;
    let tuple = interner.tuple(vec![
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

    let result = canon.canonicalize(tuple);
    assert_eq!(result, tuple);
}

#[test]
fn canonicalize_tuple_preserves_optional_and_rest() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();
    let mut canon = Canonicalizer::new(&interner, &env);

    use crate::types::TupleElement;
    let tuple = interner.tuple(vec![
        TupleElement {
            type_id: TypeId::STRING,
            name: None,
            optional: true,
            rest: false,
        },
        TupleElement {
            type_id: TypeId::NUMBER,
            name: None,
            optional: false,
            rest: true,
        },
    ]);

    let result = canon.canonicalize(tuple);
    // Look up the result's tuple elements
    if let Some(TypeData::Tuple(list_id)) = interner.lookup(result) {
        let elements = interner.tuple_list(list_id);
        assert_eq!(elements.len(), 2);
        assert!(elements[0].optional);
        assert!(!elements[0].rest);
        assert!(!elements[1].optional);
        assert!(elements[1].rest);
    } else {
        panic!("Expected tuple type");
    }
}

// ===================================================================
// Union canonicalization (commutativity)
// ===================================================================

#[test]
fn canonicalize_union_commutativity() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();

    // Create unions with members in different orders
    let union_ab = interner.union_preserve_members(vec![TypeId::STRING, TypeId::NUMBER]);
    let union_ba = interner.union_preserve_members(vec![TypeId::NUMBER, TypeId::STRING]);

    let mut canon1 = Canonicalizer::new(&interner, &env);
    let result1 = canon1.canonicalize(union_ab);

    let mut canon2 = Canonicalizer::new(&interner, &env);
    let result2 = canon2.canonicalize(union_ba);

    // Both orderings should produce the same canonical form
    assert_eq!(
        result1, result2,
        "Union(A, B) and Union(B, A) should canonicalize identically"
    );
}

#[test]
fn canonicalize_union_deduplication() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();
    let mut canon = Canonicalizer::new(&interner, &env);

    // Union with duplicates: string | number | string
    let union =
        interner.union_preserve_members(vec![TypeId::STRING, TypeId::NUMBER, TypeId::STRING]);
    let result = canon.canonicalize(union);

    // Should be deduplicated
    if let Some(TypeData::Union(list_id)) = interner.lookup(result) {
        let members = interner.type_list(list_id);
        assert_eq!(members.len(), 2, "Duplicate members should be deduplicated");
    }
    // (If the interner already normalizes, the input union may already be 2 members)
}

#[test]
fn canonicalize_union_three_members() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();

    let u1 = interner.union_preserve_members(vec![TypeId::STRING, TypeId::NUMBER, TypeId::BOOLEAN]);
    let u2 = interner.union_preserve_members(vec![TypeId::BOOLEAN, TypeId::STRING, TypeId::NUMBER]);

    let mut c1 = Canonicalizer::new(&interner, &env);
    let mut c2 = Canonicalizer::new(&interner, &env);

    assert_eq!(
        c1.canonicalize(u1),
        c2.canonicalize(u2),
        "Three-member unions should canonicalize identically regardless of order"
    );
}

// ===================================================================
// Intersection canonicalization
// ===================================================================

#[test]
fn canonicalize_intersection_sorts_structural_members() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();

    // Use type parameters to create intersections that won't be reduced
    let t = interner.type_param(TypeParamInfo {
        name: interner.intern_string("T"),
        constraint: None,
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    });
    let u = interner.type_param(TypeParamInfo {
        name: interner.intern_string("U"),
        constraint: None,
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    });

    // T & U vs U & T — both should produce the same canonical intersection
    // since both are structural (non-callable) types
    let inter1 = interner.intersection(vec![t, u]);
    let inter2 = interner.intersection(vec![u, t]);

    let mut c1 = Canonicalizer::new(&interner, &env);
    let mut c2 = Canonicalizer::new(&interner, &env);

    assert_eq!(
        c1.canonicalize(inter1),
        c2.canonicalize(inter2),
        "Intersection(T, U) and Intersection(U, T) should canonicalize identically for structural types"
    );
}

// ===================================================================
// Function canonicalization
// ===================================================================

#[test]
fn canonicalize_function_type() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();
    let mut canon = Canonicalizer::new(&interner, &env);

    use crate::types::{FunctionShape, ParamInfo};
    let func = interner.function(FunctionShape {
        type_params: vec![],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: TypeId::STRING,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::NUMBER,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let result = canon.canonicalize(func);
    // Function with only primitive types should be the same
    if let Some(TypeData::Function(shape_id)) = interner.lookup(result) {
        let shape = interner.function_shape(shape_id);
        assert_eq!(shape.return_type, TypeId::NUMBER);
        assert_eq!(shape.params.len(), 1);
        assert_eq!(shape.params[0].type_id, TypeId::STRING);
    } else {
        panic!("Expected function type");
    }
}

#[test]
fn canonicalize_function_with_type_params_uses_bound_parameter() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();

    use crate::types::{FunctionShape, ParamInfo};

    // (x: T) => T  with param named "T"
    let t_atom = interner.intern_string("T");
    let t_param = interner.type_param(TypeParamInfo {
        name: t_atom,
        constraint: None,
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    });
    let func_t = interner.function(FunctionShape {
        type_params: vec![TypeParamInfo {
            name: t_atom,
            constraint: None,
            default: None,
            is_const: false,
            variance: crate::TypeParamVariance::None,
        }],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: t_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_param,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let mut canon = Canonicalizer::new(&interner, &env);
    let result = canon.canonicalize(func_t);

    // The canonicalized function should use BoundParameter(0) for T
    if let Some(TypeData::Function(shape_id)) = interner.lookup(result) {
        let shape = interner.function_shape(shape_id);
        // The param type and return type should both be BoundParameter(0)
        assert!(
            matches!(
                interner.lookup(shape.params[0].type_id),
                Some(TypeData::BoundParameter(0))
            ),
            "Param type should be BoundParameter(0), got: {:?}",
            interner.lookup(shape.params[0].type_id)
        );
        assert!(
            matches!(
                interner.lookup(shape.return_type),
                Some(TypeData::BoundParameter(0))
            ),
            "Return type should be BoundParameter(0), got: {:?}",
            interner.lookup(shape.return_type)
        );
    } else {
        panic!("Expected function type, got: {:?}", interner.lookup(result));
    }
}

#[test]
fn canonicalize_function_type_params_name_preserved_in_shape() {
    // Note: The canonicalizer preserves type parameter names in function shapes
    // (unlike mapped types where the name is erased). This means two functions
    // with different type param names but same structure will have different
    // canonical TypeIds. Full alpha-equivalence for functions would require
    // erasing names in the TypeParamInfo as well.
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();

    use crate::types::{FunctionShape, ParamInfo};

    let t_atom = interner.intern_string("T");
    let t_param = interner.type_param(TypeParamInfo {
        name: t_atom,
        constraint: None,
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    });
    let func_t = interner.function(FunctionShape {
        type_params: vec![TypeParamInfo {
            name: t_atom,
            constraint: None,
            default: None,
            is_const: false,
            variance: crate::TypeParamVariance::None,
        }],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: t_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: t_param,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let u_atom = interner.intern_string("U");
    let u_param = interner.type_param(TypeParamInfo {
        name: u_atom,
        constraint: None,
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    });
    let func_u = interner.function(FunctionShape {
        type_params: vec![TypeParamInfo {
            name: u_atom,
            constraint: None,
            default: None,
            is_const: false,
            variance: crate::TypeParamVariance::None,
        }],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: u_param,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: u_param,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let mut c1 = Canonicalizer::new(&interner, &env);
    let mut c2 = Canonicalizer::new(&interner, &env);
    let r1 = c1.canonicalize(func_t);
    let r2 = c2.canonicalize(func_u);

    // Due to type param name preservation, these produce different canonical forms
    // Both use BoundParameter(0) in body, but the TypeParamInfo name differs
    assert_ne!(
        r1, r2,
        "Functions with different type param names have different canonical forms \
         (name is preserved in function shapes, unlike mapped types)"
    );
}

// ===================================================================
// Object canonicalization
// ===================================================================

#[test]
fn canonicalize_object_with_primitives() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();
    let mut canon = Canonicalizer::new(&interner, &env);

    let obj = interner.object(vec![
        PropertyInfo::new(interner.intern_string("x"), TypeId::NUMBER),
        PropertyInfo::new(interner.intern_string("y"), TypeId::STRING),
    ]);

    let result = canon.canonicalize(obj);
    if let Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) =
        interner.lookup(result)
    {
        let shape = interner.object_shape(shape_id);
        assert_eq!(shape.properties.len(), 2);
    } else {
        panic!("Expected object type");
    }
}

#[test]
fn canonicalize_empty_object() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();
    let mut canon = Canonicalizer::new(&interner, &env);

    let obj = interner.object(vec![]);
    let result = canon.canonicalize(obj);
    if let Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) =
        interner.lookup(result)
    {
        let shape = interner.object_shape(shape_id);
        assert!(shape.properties.is_empty());
    } else {
        panic!("Expected object type");
    }
}

// ===================================================================
// Application (generic) canonicalization
// ===================================================================

#[test]
fn canonicalize_application_with_primitive_args() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();
    let mut canon = Canonicalizer::new(&interner, &env);

    // Simulate Array<number> as Application(Lazy(DefId(1)), [number])
    let base = interner.lazy(DefId(1));
    let app = interner.application(base, vec![TypeId::NUMBER]);

    let result = canon.canonicalize(app);
    if let Some(TypeData::Application(app_id)) = interner.lookup(result) {
        let app_data = interner.type_application(app_id);
        // Args should still be [number]
        assert_eq!(app_data.args.len(), 1);
        assert_eq!(app_data.args[0], TypeId::NUMBER);
    } else {
        panic!("Expected application type");
    }
}

// ===================================================================
// Template literal canonicalization
// ===================================================================

#[test]
fn canonicalize_template_literal_with_primitive_type() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();
    let mut canon = Canonicalizer::new(&interner, &env);

    use crate::types::TemplateSpan;
    let tl = interner.template_literal(vec![
        TemplateSpan::Text(interner.intern_string("hello ")),
        TemplateSpan::Type(TypeId::STRING),
    ]);

    let result = canon.canonicalize(tl);
    if let Some(TypeData::TemplateLiteral(id)) = interner.lookup(result) {
        let spans = interner.template_list(id);
        assert_eq!(spans.len(), 2);
    } else {
        panic!("Expected template literal type");
    }
}

// ===================================================================
// String intrinsic canonicalization
// ===================================================================

#[test]
fn canonicalize_string_intrinsic() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();
    let mut canon = Canonicalizer::new(&interner, &env);

    use crate::types::StringIntrinsicKind;
    let upper = interner.string_intrinsic(StringIntrinsicKind::Uppercase, TypeId::STRING);

    let result = canon.canonicalize(upper);
    if let Some(TypeData::StringIntrinsic { kind, type_arg }) = interner.lookup(result) {
        assert_eq!(kind, StringIntrinsicKind::Uppercase);
        assert_eq!(type_arg, TypeId::STRING);
    } else {
        panic!("Expected string intrinsic type");
    }
}

#[test]
fn canonicalize_string_intrinsic_in_function_uses_bound_parameter() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();

    use crate::types::{FunctionShape, ParamInfo, StringIntrinsicKind};

    // Uppercase<T> in function <T>(x: Uppercase<T>) => void
    let t_atom = interner.intern_string("T");
    let t_param = interner.type_param(TypeParamInfo {
        name: t_atom,
        constraint: None,
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    });
    let upper_t = interner.string_intrinsic(StringIntrinsicKind::Uppercase, t_param);
    let func_t = interner.function(FunctionShape {
        type_params: vec![TypeParamInfo {
            name: t_atom,
            constraint: None,
            default: None,
            is_const: false,
            variance: crate::TypeParamVariance::None,
        }],
        params: vec![ParamInfo {
            name: Some(interner.intern_string("x")),
            type_id: upper_t,
            optional: false,
            rest: false,
        }],
        this_type: None,
        return_type: TypeId::VOID,
        type_predicate: None,
        is_constructor: false,
        is_method: false,
    });

    let mut canon = Canonicalizer::new(&interner, &env);
    let result = canon.canonicalize(func_t);

    // The canonicalized function should have Uppercase<BoundParameter(0)> as param type
    if let Some(TypeData::Function(shape_id)) = interner.lookup(result) {
        let shape = interner.function_shape(shape_id);
        let param_type = shape.params[0].type_id;
        // The param should be StringIntrinsic(Uppercase, BoundParameter(0))
        if let Some(TypeData::StringIntrinsic { kind, type_arg }) = interner.lookup(param_type) {
            assert_eq!(kind, StringIntrinsicKind::Uppercase);
            assert!(
                matches!(interner.lookup(type_arg), Some(TypeData::BoundParameter(0))),
                "Intrinsic arg should be BoundParameter(0), got: {:?}",
                interner.lookup(type_arg)
            );
        } else {
            panic!(
                "Expected StringIntrinsic param type, got: {:?}",
                interner.lookup(param_type)
            );
        }
    } else {
        panic!("Expected function type");
    }
}

// ===================================================================
// Mapped type canonicalization (alpha-equivalence)
// ===================================================================

#[test]
fn canonicalize_mapped_type_alpha_equivalence() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();

    use crate::types::MappedType;

    // { [K in string]: number }
    let k_atom = interner.intern_string("K");
    let k_param = interner.type_param(TypeParamInfo {
        name: k_atom,
        constraint: None,
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    });
    let mapped_k = interner.mapped(MappedType {
        type_param: TypeParamInfo {
            name: k_atom,
            constraint: None,
            default: None,
            is_const: false,
            variance: crate::TypeParamVariance::None,
        },
        constraint: TypeId::STRING,
        template: k_param,
        name_type: None,
        readonly_modifier: None,
        optional_modifier: None,
    });

    // { [P in string]: number }
    let p_atom = interner.intern_string("P");
    let p_param = interner.type_param(TypeParamInfo {
        name: p_atom,
        constraint: None,
        default: None,
        is_const: false,
        variance: crate::TypeParamVariance::None,
    });
    let mapped_p = interner.mapped(MappedType {
        type_param: TypeParamInfo {
            name: p_atom,
            constraint: None,
            default: None,
            is_const: false,
            variance: crate::TypeParamVariance::None,
        },
        constraint: TypeId::STRING,
        template: p_param,
        name_type: None,
        readonly_modifier: None,
        optional_modifier: None,
    });

    let mut c1 = Canonicalizer::new(&interner, &env);
    let mut c2 = Canonicalizer::new(&interner, &env);
    assert_eq!(
        c1.canonicalize(mapped_k),
        c2.canonicalize(mapped_p),
        "{{ [K in string]: K }} and {{ [P in string]: P }} should be alpha-equivalent"
    );
}

// ===================================================================
// Expanding alias chain termination
// ===================================================================

struct ExpandingAliasResolver;

impl TypeResolver for ExpandingAliasResolver {
    fn resolve_ref(&self, _symbol: SymbolRef, _interner: &dyn TypeDatabase) -> Option<TypeId> {
        None
    }

    fn resolve_lazy(&self, def_id: DefId, interner: &dyn TypeDatabase) -> Option<TypeId> {
        Some(interner.lazy(DefId(def_id.0 + 1)))
    }

    fn get_def_kind(&self, _def_id: DefId) -> Option<DefKind> {
        Some(DefKind::TypeAlias)
    }
}

#[test]
fn test_canonicalize_expanding_alias_chain_terminates() {
    let interner = TypeInterner::new();
    let resolver = ExpandingAliasResolver;
    let mut canon = Canonicalizer::new(&interner, &resolver);

    let start = interner.lazy(DefId(1));
    let result = canon.canonicalize(start);

    assert!(
        matches!(interner.lookup(result), Some(TypeData::Lazy(_))),
        "canonicalization should terminate with a lazy fallback for expanding aliases"
    );
}

// ===================================================================
// Self-referential type alias (Recursive index)
// ===================================================================

/// Resolver where DefId(1) is a type alias whose body is { value: DefId(1) }
/// i.e., type Node = { value: Node }
struct SelfReferentialResolver {
    body: std::cell::RefCell<Option<TypeId>>,
}

impl SelfReferentialResolver {
    fn new() -> Self {
        Self {
            body: std::cell::RefCell::new(None),
        }
    }

    fn set_body(&self, type_id: TypeId) {
        *self.body.borrow_mut() = Some(type_id);
    }
}

impl TypeResolver for SelfReferentialResolver {
    fn resolve_ref(&self, _symbol: SymbolRef, _interner: &dyn TypeDatabase) -> Option<TypeId> {
        None
    }

    fn resolve_lazy(&self, def_id: DefId, _interner: &dyn TypeDatabase) -> Option<TypeId> {
        if def_id == DefId(1) {
            *self.body.borrow()
        } else {
            None
        }
    }

    fn get_def_kind(&self, def_id: DefId) -> Option<DefKind> {
        if def_id == DefId(1) {
            Some(DefKind::TypeAlias)
        } else {
            None
        }
    }
}

#[test]
fn canonicalize_self_referential_alias_via_different_type_ids() {
    let interner = TypeInterner::new();
    let resolver = SelfReferentialResolver::new();

    // type Node = { value: Node }
    // The body is an object whose property references Lazy(DefId(1)).
    // When Lazy(DefId(1)) is the top-level input, the TypeId-level guard
    // detects the cycle (same TypeId visited twice) and returns the Lazy
    // type as-is. The def_stack-based Recursive(n) detection only triggers
    // when the self-reference goes through a DIFFERENT TypeId path.
    let lazy_self = interner.lazy(DefId(1));
    let obj = interner.object(vec![PropertyInfo::new(
        interner.intern_string("value"),
        lazy_self,
    )]);
    resolver.set_body(obj);

    let mut canon = Canonicalizer::new(&interner, &resolver);
    let result = canon.canonicalize(lazy_self);

    // The result is an object where the self-referencing property retains
    // its Lazy(DefId(1)) type because the TypeId guard caught the cycle
    // before def_stack-level Recursive index generation could fire.
    if let Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) =
        interner.lookup(result)
    {
        let shape = interner.object_shape(shape_id);
        assert_eq!(shape.properties.len(), 1);
        let prop_type = shape.properties[0].type_id;
        // The guard returned the Lazy type on cycle detection
        assert!(
            matches!(interner.lookup(prop_type), Some(TypeData::Lazy(_))),
            "Self-referencing property retains Lazy type via guard cycle detection, got: {:?}",
            interner.lookup(prop_type)
        );
    } else {
        panic!("Expected object type, got: {:?}", interner.lookup(result));
    }
}

// ===================================================================
// Nominal types (Interface) preserved as Lazy
// ===================================================================

struct NominalResolver;

impl TypeResolver for NominalResolver {
    fn resolve_ref(&self, _symbol: SymbolRef, _interner: &dyn TypeDatabase) -> Option<TypeId> {
        None
    }

    fn resolve_lazy(&self, _def_id: DefId, _interner: &dyn TypeDatabase) -> Option<TypeId> {
        None
    }

    fn get_def_kind(&self, _def_id: DefId) -> Option<DefKind> {
        // All DefIds are interfaces (nominal)
        Some(DefKind::Interface)
    }
}

#[test]
fn canonicalize_nominal_type_preserved() {
    let interner = TypeInterner::new();
    let resolver = NominalResolver;
    let mut canon = Canonicalizer::new(&interner, &resolver);

    let lazy = interner.lazy(DefId(42));
    let result = canon.canonicalize(lazy);

    // Nominal type should be preserved as-is
    assert_eq!(
        result, lazy,
        "Nominal (Interface) type should remain as Lazy(DefId)"
    );
}

// ===================================================================
// Caching
// ===================================================================

#[test]
fn canonicalize_caches_results() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();
    let mut canon = Canonicalizer::new(&interner, &env);

    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let result1 = canon.canonicalize(union);
    let result2 = canon.canonicalize(union);

    // Second call should use cache and return same result
    assert_eq!(result1, result2);
}

// ===================================================================
// Nested composite types
// ===================================================================

#[test]
fn canonicalize_array_of_union() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();
    let mut canon = Canonicalizer::new(&interner, &env);

    // (string | number)[]
    let union = interner.union(vec![TypeId::STRING, TypeId::NUMBER]);
    let arr = interner.array(union);
    let result = canon.canonicalize(arr);

    if let Some(TypeData::Array(elem)) = interner.lookup(result) {
        // Element should be canonicalized union
        assert!(matches!(interner.lookup(elem), Some(TypeData::Union(_))));
    } else {
        panic!("Expected array type");
    }
}

#[test]
fn canonicalize_union_of_arrays() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();
    let mut canon = Canonicalizer::new(&interner, &env);

    // string[] | number[]
    let str_arr = interner.array(TypeId::STRING);
    let num_arr = interner.array(TypeId::NUMBER);
    let union = interner.union(vec![str_arr, num_arr]);
    let result = canon.canonicalize(union);

    if let Some(TypeData::Union(list_id)) = interner.lookup(result) {
        let members = interner.type_list(list_id);
        assert_eq!(members.len(), 2);
    } else {
        panic!("Expected union type");
    }
}

// ===================================================================
// Conditional type (passthrough)
// ===================================================================

#[test]
fn canonicalize_conditional_passthrough() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();
    let mut canon = Canonicalizer::new(&interner, &env);

    use crate::types::ConditionalType;
    let cond = interner.conditional(ConditionalType {
        check_type: TypeId::STRING,
        extends_type: TypeId::NUMBER,
        true_type: TypeId::BOOLEAN,
        false_type: TypeId::NEVER,
        is_distributive: false,
    });

    let result = canon.canonicalize(cond);
    // Conditional types fall through to the default case (preserved as-is)
    // since there's no explicit match arm for them in the canonicalizer
    assert_eq!(result, cond);
}

// ===================================================================
// Index access and keyof (passthrough)
// ===================================================================

#[test]
fn canonicalize_index_access_passthrough() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();
    let mut canon = Canonicalizer::new(&interner, &env);

    let idx = interner.index_access(TypeId::STRING, TypeId::NUMBER);
    let result = canon.canonicalize(idx);
    // IndexAccess falls through to default case
    assert_eq!(result, idx);
}

#[test]
fn canonicalize_keyof_passthrough() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();
    let mut canon = Canonicalizer::new(&interner, &env);

    let keyof = interner.keyof(TypeId::STRING);
    let result = canon.canonicalize(keyof);
    // KeyOf falls through to default case
    assert_eq!(result, keyof);
}

// ===================================================================
// Object with index signatures
// ===================================================================

#[test]
fn canonicalize_object_with_index_signature() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();
    let mut canon = Canonicalizer::new(&interner, &env);

    use crate::types::{IndexSignature, ObjectShape};
    let shape = ObjectShape {
        properties: vec![],
        string_index: Some(IndexSignature {
            key_type: TypeId::STRING,
            value_type: TypeId::NUMBER,
            readonly: false,
            param_name: None,
        }),
        number_index: None,
        symbol: None,
        flags: Default::default(),
    };
    let obj = interner.object_with_index(shape);

    let result = canon.canonicalize(obj);
    if let Some(TypeData::Object(shape_id) | TypeData::ObjectWithIndex(shape_id)) =
        interner.lookup(result)
    {
        let shape = interner.object_shape(shape_id);
        assert!(shape.string_index.is_some());
        let idx = shape.string_index.as_ref().unwrap();
        assert_eq!(idx.key_type, TypeId::STRING);
        assert_eq!(idx.value_type, TypeId::NUMBER);
    } else {
        panic!("Expected object type with index");
    }
}
