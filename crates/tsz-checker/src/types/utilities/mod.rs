//! Parameter type utilities, type construction, and type resolution methods
//! for `CheckerState`.

pub(crate) mod class_navigation_helpers;
pub(crate) mod const_enum_eval;
pub(crate) mod contextual_calls;
pub(crate) mod contextual_parameters;
pub(crate) mod core;
pub(crate) mod cycle_guard;
pub(crate) mod element_indexable;
pub(crate) mod enum_eval;
pub(crate) mod enum_utils;
pub(crate) mod enum_utils_readonly;
pub(crate) mod fresh_literal;
pub(crate) mod heritage_walk_state;
pub(crate) mod mutable_binding_nullish;
pub(crate) mod overlap_relation_helpers;
pub(crate) mod return_type;
pub(crate) mod return_type_any_assertion;
pub(crate) mod return_type_noinfer_widening;
pub(crate) mod return_type_nullish;
pub(crate) mod widening;
