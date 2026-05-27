use super::*;
use crate::construction::TypeInterner;
use crate::def::DefId;
use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};
use crate::relations::subtype::SubtypeChecker;

fn test_type_param(interner: &TypeInterner, name: &str) -> (Atom, TypeId) {
    let name = interner.intern_string(name);
    (name, test_type_param_from_name(interner, name))
}

fn test_infer_param(interner: &TypeInterner, name: &str) -> (Atom, TypeId) {
    let name = interner.intern_string(name);
    (name, test_infer_param_from_name(interner, name))
}

fn test_type_param_from_name(interner: &TypeInterner, name: Atom) -> TypeId {
    interner.intern(TypeData::TypeParameter(TypeParamInfo::simple(name)))
}

fn test_infer_param_from_name(interner: &TypeInterner, name: Atom) -> TypeId {
    interner.intern(TypeData::Infer(TypeParamInfo::simple(name)))
}

// Split into under-cap shards by evaluator family while preserving test order.
include!("evaluate_tests_parts/conditional_infer_core.rs");
include!("evaluate_tests_parts/conditional_infer_arrays.rs");
include!("evaluate_tests_parts/conditional_infer_objects.rs");
include!("evaluate_tests_parts/conditional_infer_templates.rs");
include!("evaluate_tests_parts/conditional_infer_functions.rs");
include!("evaluate_tests_parts/indexed_access_conditionals.rs");
include!("evaluate_tests_parts/keyof_indexed_access.rs");
include!("evaluate_tests_parts/mapped_core.rs");
include!("evaluate_tests_parts/application_conditionals.rs");
include!("evaluate_tests_parts/application_template_literal.rs");
include!("evaluate_tests_parts/template_literal_indexed_access.rs");
include!("evaluate_tests_parts/infer_utility_core.rs");
include!("evaluate_tests_parts/infer_utility_iterables.rs");
include!("evaluate_tests_parts/utility_mapped_infer.rs");
include!("evaluate_tests_parts/template_literal_utility.rs");
include!("evaluate_tests_parts/recursive_infer_awaited.rs");
include!("evaluate_tests_parts/template_literal_intrinsics.rs");
include!("evaluate_tests_parts/literal_satisfies_intrinsics.rs");
include!("evaluate_tests_parts/mapped_utility_distribution.rs");
include!("evaluate_tests_parts/distributive_conditionals.rs");
include!("evaluate_tests_parts/utility_return_parameters.rs");
include!("evaluate_tests_parts/template_literal_infer_distribution.rs");
include!("evaluate_tests_parts/mapped_keyof_template.rs");
include!("evaluate_tests_parts/indexed_access_template_recursive.rs");
include!("evaluate_tests_parts/tuple_keyof_indexed_access.rs");
