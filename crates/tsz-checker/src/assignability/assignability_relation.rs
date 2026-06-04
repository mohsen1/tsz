use crate::query_boundaries::assignability::{
    AssignabilityQueryInputs, RelationOutcome, RelationRequest, are_types_overlapping_with_env,
    assignability_cache_key, check_application_variance_assignability, get_allowed_keys,
    get_keyof_type, get_string_literal_value, get_union_members,
    intersection_source_has_target_constituent, is_assignable_bivariant_with_resolver,
    is_assignable_with_overrides, is_relation_cacheable, object_shape_for_type,
};

use crate::query_boundaries::common::{
    intersection_members, object_shape_id, object_with_index_shape_id, union_members,
};

use crate::query_boundaries::state::type_resolution::{get_application_info, get_lazy_def_id};

use crate::state::{CheckerOverrideProvider, CheckerState};

use rustc_hash::FxHashSet;

use tracing::trace;

use tsz_solver::TypeId;

use tsz_solver::computation::TypeResolver;

include!("assignability_relation_parts/part1.rs");
include!("assignability_relation_parts/part2.rs");
