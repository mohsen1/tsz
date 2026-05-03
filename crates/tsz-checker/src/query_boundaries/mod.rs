// Allow dead code and related lints in scaffolding modules that define
// the unified relation boundary API (NORTH_STAR.md §22). These types
// will be wired in as checker paths migrate to the boundary API.
#[allow(dead_code, clippy::missing_const_for_fn, clippy::match_same_arms)]
pub(crate) mod assignability;
#[allow(
    dead_code,
    clippy::missing_const_for_fn,
    clippy::match_same_arms,
    clippy::doc_markdown,
    clippy::manual_map
)]
pub(crate) mod capabilities;
#[allow(
    dead_code,
    clippy::missing_const_for_fn,
    clippy::match_same_arms,
    clippy::doc_markdown,
    clippy::manual_map
)]
pub(crate) mod checkers;
#[allow(
    dead_code,
    clippy::missing_const_for_fn,
    clippy::match_same_arms,
    clippy::doc_markdown,
    clippy::manual_map
)]
pub(crate) mod class;
#[allow(
    dead_code,
    clippy::missing_const_for_fn,
    clippy::match_same_arms,
    clippy::doc_markdown,
    clippy::manual_map
)]
pub(crate) mod class_type;
#[allow(
    dead_code,
    clippy::missing_const_for_fn,
    clippy::match_same_arms,
    clippy::doc_markdown,
    clippy::manual_map
)]
pub(crate) mod common;
#[allow(
    dead_code,
    clippy::missing_const_for_fn,
    clippy::match_same_arms,
    clippy::doc_markdown,
    clippy::manual_map
)]
pub(crate) mod definite_assignment;
#[allow(
    dead_code,
    clippy::missing_const_for_fn,
    clippy::match_same_arms,
    clippy::doc_markdown,
    clippy::manual_map
)]
pub(crate) mod diagnostics;
#[allow(
    dead_code,
    clippy::missing_const_for_fn,
    clippy::match_same_arms,
    clippy::doc_markdown,
    clippy::manual_map
)]
pub(crate) mod dispatch;
#[allow(
    dead_code,
    clippy::missing_const_for_fn,
    clippy::match_same_arms,
    clippy::doc_markdown,
    clippy::manual_map
)]
#[allow(private_interfaces)]
pub(crate) mod environment;
#[allow(
    dead_code,
    clippy::missing_const_for_fn,
    clippy::match_same_arms,
    clippy::doc_markdown,
    clippy::manual_map
)]
pub(crate) mod flow;
#[allow(
    dead_code,
    clippy::missing_const_for_fn,
    clippy::match_same_arms,
    clippy::doc_markdown,
    clippy::manual_map
)]
pub(crate) mod flow_analysis;
#[allow(dead_code)]
pub(crate) mod index_signature;
#[allow(dead_code)]
pub(crate) mod inference;
#[allow(
    dead_code,
    clippy::missing_const_for_fn,
    clippy::match_same_arms,
    clippy::doc_markdown,
    clippy::manual_map
)]
pub(crate) mod js_exports;
#[allow(dead_code)]
pub(crate) mod name_resolution;
#[allow(
    dead_code,
    clippy::missing_const_for_fn,
    clippy::match_same_arms,
    clippy::doc_markdown,
    clippy::manual_map
)]
pub(crate) mod property_access;
#[allow(dead_code, clippy::missing_const_for_fn, clippy::match_same_arms)]
pub(crate) mod relation_types;
#[allow(
    dead_code,
    clippy::missing_const_for_fn,
    clippy::match_same_arms,
    clippy::doc_markdown,
    clippy::manual_map
)]
pub(crate) mod state;
#[allow(
    dead_code,
    clippy::missing_const_for_fn,
    clippy::match_same_arms,
    clippy::doc_markdown,
    clippy::manual_map
)]
pub(crate) mod type_checking;
#[allow(
    dead_code,
    clippy::missing_const_for_fn,
    clippy::match_same_arms,
    clippy::doc_markdown,
    clippy::manual_map
)]
pub(crate) mod type_checking_utilities;
#[allow(
    dead_code,
    clippy::missing_const_for_fn,
    clippy::match_same_arms,
    clippy::doc_markdown,
    clippy::manual_map
)]
pub(crate) mod type_computation;
#[allow(
    dead_code,
    clippy::missing_const_for_fn,
    clippy::match_same_arms,
    clippy::doc_markdown,
    clippy::manual_map
)]
pub(crate) mod type_construction;
#[allow(
    dead_code,
    clippy::missing_const_for_fn,
    clippy::match_same_arms,
    clippy::doc_markdown,
    clippy::manual_map
)]
pub(crate) mod variance;
#[allow(dead_code)]
pub(crate) mod widening;
