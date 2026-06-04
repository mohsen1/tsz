use super::*;

use crate::evaluation::evaluate_rules::apparent::make_apparent_method_type;

use crate::instantiation::instantiate::{
    TypeSubstitution, instantiate_type_cached, instantiate_type_with_infer_cached,
    substitute_this_type_cached,
};

use crate::objects::apparent_primitive_member_kind;

use crate::types::{
    MappedType, MappedTypeId, PropertyInfo, PropertyLookup, TupleElement, TypeApplicationId,
    TypeParamInfo,
};

fn is_array_mutating_method(prop_name: &str) -> bool {
    matches!(
        prop_name,
        "copyWithin"
            | "fill"
            | "pop"
            | "push"
            | "reverse"
            | "shift"
            | "sort"
            | "splice"
            | "unshift"
    )
}

include!("property_helpers_parts/part1.rs");
include!("property_helpers_parts/part2.rs");
