// Allow dead code and related lints in scaffolding modules that define
// the unified relation boundary API (NORTH_STAR.md §22). These types
// will be wired in as checker paths migrate to the boundary API.
#[allow(dead_code, clippy::missing_const_for_fn, clippy::match_same_arms)]
pub(crate) mod assignability;
pub(crate) mod capabilities;
pub(crate) mod checkers;
pub(crate) mod class;
pub(crate) mod class_type;
pub(crate) mod common;
pub(crate) mod definite_assignment;
pub(crate) mod diagnostics;
pub(crate) mod dispatch;
pub(crate) mod environment;
pub(crate) mod flow;
pub(crate) mod flow_analysis;
pub(crate) mod js_exports;
#[allow(dead_code)]
pub(crate) mod name_resolution;
pub(crate) mod property_access;
#[allow(dead_code, clippy::missing_const_for_fn, clippy::match_same_arms)]
pub(crate) mod relation_types;
pub(crate) mod state;
pub(crate) mod type_checking;
pub(crate) mod type_checking_utilities;
pub(crate) mod type_computation;
pub(crate) mod type_construction;
