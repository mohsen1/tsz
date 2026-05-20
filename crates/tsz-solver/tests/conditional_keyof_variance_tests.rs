use crate::relations::subtype::TypeEnvironment;
use crate::relations::variance::compute_variance_with_resolver;
use crate::types::Variance;
use crate::{ConditionalType, TypeId, TypeInterner, TypeParamInfo};

#[test]
fn conditional_keyof_branch_variance_computation() {
    let interner = TypeInterner::new();
    let env = TypeEnvironment::new();

    let t_name = interner.intern_string("T");
    let t_type = interner.type_param(TypeParamInfo {
        name: t_name,
        constraint: None,
        default: None,
        is_const: false,
    });
    let keyof_t = interner.keyof(t_type);

    let covariant = interner.conditional(ConditionalType {
        check_type: t_type,
        extends_type: TypeId::STRING,
        true_type: t_type,
        false_type: TypeId::NUMBER,
        is_distributive: true,
    });
    let contravariant = interner.conditional(ConditionalType {
        check_type: t_type,
        extends_type: TypeId::STRING,
        true_type: keyof_t,
        false_type: TypeId::NUMBER,
        is_distributive: true,
    });
    let invariant = interner.conditional(ConditionalType {
        check_type: t_type,
        extends_type: TypeId::STRING,
        true_type: keyof_t,
        false_type: t_type,
        is_distributive: true,
    });
    let covariant_false = interner.conditional(ConditionalType {
        check_type: t_type,
        extends_type: TypeId::STRING,
        true_type: TypeId::NUMBER,
        false_type: t_type,
        is_distributive: true,
    });
    let contravariant_false = interner.conditional(ConditionalType {
        check_type: t_type,
        extends_type: TypeId::STRING,
        true_type: TypeId::NUMBER,
        false_type: keyof_t,
        is_distributive: true,
    });
    let invariant_false = interner.conditional(ConditionalType {
        check_type: t_type,
        extends_type: TypeId::STRING,
        true_type: t_type,
        false_type: keyof_t,
        is_distributive: true,
    });

    assert_eq!(
        compute_variance_with_resolver(&interner, &env, covariant, t_name),
        Variance::COVARIANT | Variance::DIRECT_USAGE
    );
    assert_eq!(
        compute_variance_with_resolver(&interner, &env, contravariant, t_name),
        Variance::CONTRAVARIANT | Variance::DIRECT_USAGE
    );
    assert_eq!(
        compute_variance_with_resolver(&interner, &env, invariant, t_name),
        Variance::COVARIANT | Variance::CONTRAVARIANT | Variance::DIRECT_USAGE
    );
    assert_eq!(
        compute_variance_with_resolver(&interner, &env, covariant_false, t_name),
        Variance::COVARIANT | Variance::DIRECT_USAGE
    );
    assert_eq!(
        compute_variance_with_resolver(&interner, &env, contravariant_false, t_name),
        Variance::CONTRAVARIANT | Variance::DIRECT_USAGE
    );
    assert_eq!(
        compute_variance_with_resolver(&interner, &env, invariant_false, t_name),
        Variance::COVARIANT | Variance::CONTRAVARIANT | Variance::DIRECT_USAGE
    );
}
