//! Class instance type resolution (instance members, inheritance, interface merging).

pub mod constructor;
mod core;
mod core_heritage_merge;
mod entry;
mod helpers;
mod heritage_identity;
mod js_class_properties;
mod prescan;

pub(super) use helpers::can_skip_base_instantiation;
