//! Class instance type resolution (instance members, inheritance, interface merging).

pub mod constructor;
mod core;
mod entry;
mod helpers;
mod heritage_identity;
mod instance;
mod instance_merge;
mod js_class_properties;
mod prescan;
mod walk_state;

pub(super) use helpers::can_skip_base_instantiation;
pub(crate) use helpers::{
    class_self_reference_deferral_seen, note_class_self_reference_deferral,
    replace_class_self_reference_deferral_seen,
};
