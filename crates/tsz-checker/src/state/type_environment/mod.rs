//! Type environment building, application type evaluation, property access
//! type resolution, and type node resolution.

mod app_canon_arg_identity;
mod application;
mod core;
mod def_type_resolution;
mod formatting;
pub(crate) mod lazy;
mod lazy_flow_mirror;
mod lazy_fuel;
pub(crate) mod lazy_guard_state;
mod lazy_impossible_pruning;
mod property_access_visited;
mod published_program_alias;
mod source_location;
mod type_node_resolution;
mod type_params;

use app_canon_arg_identity::app_canon_arg_identity_enabled;
