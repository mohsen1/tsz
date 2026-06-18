//! Type environment building, application type evaluation, property access
//! type resolution, and type node resolution.

mod application;
mod core;
mod formatting;
pub(crate) mod lazy;
mod lazy_flow_mirror;
mod lazy_fuel;
mod property_access_visited;
mod source_location;
mod type_node_resolution;
mod type_params;
