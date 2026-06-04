//! Structural subtype dispatch extracted from `core.rs`.
//!
//! This is the body of [`SubtypeChecker::check_subtype_inner_impl`], moved
//! verbatim into a child module so the `subtype/core.rs` engine shard stays
//! under the 2000-line file-size cap (§19). `use super::*` re-exposes the
//! parent module's imports and `SubtypeChecker` so the relocation is
//! behavior-preserving.

include!("core_dispatch_large_methods/check_subtype_inner_impl_1_0.rs");

use super::*;

impl<'a, R: TypeResolver> SubtypeChecker<'a, R> {
    __tsz_split_core_dispatch_check_subtype_inner_impl_1_0!();
}
