//! Property access type resolution, global augmentation property lookup,
//! and expando function pattern detection.

mod class_recovery;
mod enum_namespace_access;
mod helpers;
mod identifier_resolution;
mod imported_array_to_enum;
pub(crate) mod known_globals;
mod nullish_access;
mod optional_chain_cache;
mod optional_fast_path;
mod partial_initializer;
mod resolve;

use crate::query_boundaries::common::OptionalPropertyChainKey;
use tsz_solver::TypeId;

struct IdentifierPropertyAccessRequest {
    object_type: TypeId,
    original_object_type: TypeId,
    display_object_type: TypeId,
    skip_flow_narrowing: bool,
    skip_result_flow_for_result: bool,
    write_presence_only: bool,
    receiver_has_daa_error: bool,
    accessibility_error_emitted: bool,
    commonjs_named_props_disallowed: bool,
    is_this_access: bool,
    js_expando_before_assignment: bool,
}

struct OptionalPropertyChainFastPathRequest<'a> {
    object_type: TypeId,
    original_object_type: TypeId,
    question_dot_token: bool,
    skip_flow_narrowing: bool,
    skip_result_flow_for_result: bool,
    write_presence_only: bool,
    optional_property_chain_cache_key: Option<&'a OptionalPropertyChainKey>,
}

#[cfg(test)]
mod resolve_tests;
