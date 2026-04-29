//! Declaration and statement checking, including `StatementCheckCallbacks`.

pub(crate) mod class;
mod core;
pub(crate) mod directive;
mod dts_rules;
pub(crate) mod heritage;
mod heritage_class_recovery;
mod isolated_declarations;
mod js_grammar;
mod module_none;
pub(crate) mod property;
pub(crate) mod property_access;
pub(crate) mod readonly;
mod source_file;
mod strict_names;

pub(crate) use self::strict_names::is_eval_or_arguments;
pub(crate) use self::strict_names::is_strict_mode_reserved_name;
