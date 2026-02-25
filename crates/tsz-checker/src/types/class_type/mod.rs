//! Class instance type resolution (instance members, inheritance, interface merging).

pub mod constructor;
mod core;

pub(super) use core::can_skip_base_instantiation;
