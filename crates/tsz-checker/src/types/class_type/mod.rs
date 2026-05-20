//! Class instance type resolution (instance members, inheritance, interface merging).

pub mod constructor;
mod core;
mod js_class_properties;

pub(super) use core::can_skip_base_instantiation;
