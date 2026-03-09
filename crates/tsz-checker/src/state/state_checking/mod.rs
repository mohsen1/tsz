//! Declaration and statement checking, including `StatementCheckCallbacks`.

pub(crate) mod class;
mod core;
pub(crate) mod directive;
pub(crate) mod heritage;
mod hotspot_trace;
pub(crate) mod property;
pub(crate) mod readonly;

pub(crate) use self::core::is_eval_or_arguments;
pub(crate) use self::core::is_strict_mode_reserved_name;
