use crate::def::resolver::TypeResolver;

use crate::diagnostics::SubtypeFailureReason;

use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};

use crate::relations::subtype::SubtypeChecker;

use crate::type_queries::data::get_object_symbol;

use crate::types::{
    IntrinsicKind, LiteralValue, ObjectShape, ObjectShapeId, PropertyInfo, TypeId, Visibility,
};

use crate::utils;

use crate::visitor::is_type_parameter;

use crate::visitor::{
    application_id, array_element_type, callable_shape_id, function_shape_id, intrinsic_kind,
    literal_value, object_shape_id, object_with_index_shape_id, readonly_inner_type, tuple_list_id,
    union_list_id,
};

include!("explain_parts/part1.rs");
include!("explain_parts/part2.rs");
