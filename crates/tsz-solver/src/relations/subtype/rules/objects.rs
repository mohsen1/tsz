use crate::operations::iterators::get_iterator_info;

use crate::type_queries::get_return_type;

use crate::types::{
    IntrinsicKind, ObjectFlags, ObjectShape, ObjectShapeId, PropertyInfo, SymbolRef, TypeId,
    Visibility,
};

use crate::utils;

use crate::visitor::{
    application_id, lazy_def_id, object_shape_id, object_with_index_shape_id, template_literal_id,
    union_list_id,
};

use tsz_common::interner::Atom;

use super::super::{SubtypeChecker, SubtypeResult, TypeResolver};

include!("objects_parts/part1.rs");
include!("objects_parts/part2.rs");
