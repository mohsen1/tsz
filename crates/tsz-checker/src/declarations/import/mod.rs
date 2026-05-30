//! Import/export declaration validation (TS2307, TS2305, TS2309, TS1202).

mod ambient_default_dup_collect;
mod context_helpers;
pub(crate) mod core;
pub(crate) mod declaration;
pub(crate) mod declaration_attributes;
pub(crate) mod declaration_check_body;
mod declaration_helpers;
pub(crate) mod declaration_resolution;
pub(crate) mod equals;
mod exports;
mod import_alias_duplicates;
mod verbatim;
