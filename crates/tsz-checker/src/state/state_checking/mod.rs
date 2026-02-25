//! Declaration and statement checking, including `StatementCheckCallbacks`.

pub(crate) mod class;
mod core;
pub(crate) mod directive;
pub(crate) mod heritage;
pub(crate) mod property;
pub(crate) mod readonly;

pub(crate) use self::core::is_strict_mode_reserved_name;
