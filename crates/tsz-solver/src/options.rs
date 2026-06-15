//! Shared option-bag scaffolding for cache-keyed solver stages.
//!
//! Several solver stages (`evaluation`, `narrowing`, `instantiation`) carry a
//! small bag of boolean compiler-option flags that participate in their cache
//! key. Idiomatic Rust derives the `const new` / `with_*` / getter builder once
//! rather than hand-writing the same triple per stage. This module owns:
//!
//! - [`solver_options!`], a `macro_rules!` that emits that builder triple for a
//!   declared flag list. It is used by [`IndexAccessOptions`] here and by the
//!   `EvaluationOptions` / `NarrowingOptions` / `InstantiationOptions` stages.
//! - [`IndexAccessOptions`], the shared two-flag newtype embedded by both
//!   `EvaluationOptions` and `NarrowingOptions` (which carry the identical
//!   `{no_unchecked_indexed_access, exact_optional_property_types}` pair and
//!   thread it into their cache keys; see issue #10970 for why omitting a flag
//!   from a key is a correctness footgun).
//!
//! Behavior is unchanged: the generated methods keep the exact names,
//! signatures, and field order of the previous hand-written builders, and the
//! derived `Hash`/`Eq` on [`IndexAccessOptions`] hashes its two fields in the
//! same order as the old inlined `bool` pair, so cache-key hashing is
//! byte-identical.

/// Emit the `const new` / `with_*` / getter triple for an option-bag struct.
///
/// The struct itself is declared by the caller (so crate architecture guards
/// that grep for the literal `pub struct <Name>` declaration keep matching, and
/// so each stage controls its own derives and doc comment). This macro only
/// generates the `impl` block over a flat list of `bool` fields.
///
/// Each entry is `<getter> / <setter>`, where `<getter>` names a `bool` field on
/// `$ty` (and becomes the getter method) and `<setter>` is the `with_*` builder
/// method name. The setter name is supplied explicitly rather than derived,
/// because concatenating identifiers in a `macro_rules!` arm requires unstable
/// metavariable expressions. The macro generates:
///
/// - `pub const fn new() -> Self` with every flag `false`;
/// - `pub const fn <setter>(mut self, enabled: bool) -> Self` per flag;
/// - `pub const fn <getter>(self) -> bool` per flag.
macro_rules! solver_options {
    ($ty:ident { $($flag:ident / $setter:ident),+ $(,)? }) => {
        impl $ty {
            /// Construct the default option set (every flag off).
            pub const fn new() -> Self {
                Self {
                    $($flag: false,)+
                }
            }

            $(
                #[doc = concat!("Return a copy with the `", stringify!($flag), "` flag set to `enabled`.")]
                pub const fn $setter(mut self, enabled: bool) -> Self {
                    self.$flag = enabled;
                    self
                }

                #[doc = concat!("Whether the `", stringify!($flag), "` flag is set.")]
                pub const fn $flag(self) -> bool {
                    self.$flag
                }
            )+
        }
    };
}

pub(crate) use solver_options;

/// The shared `{no_unchecked_indexed_access, exact_optional_property_types}`
/// flag pair carried by both `EvaluationOptions` and `NarrowingOptions`.
///
/// Both stages are cache-keyed and both flags change which type they compute
/// (indexed access of optional/array members, homomorphic mapped-modifier
/// stripping). Embedding one newtype means a new index-access option lands in a
/// single place and cannot be silently omitted from one of the two cache keys
/// (the footgun the cache-key docs warn about for issue #10970).
///
/// `Hash`/`Eq` are derived so this newtype hashes its two `bool` fields in
/// declaration order — identical to the previously inlined `bool` pair — which
/// keeps `EvaluationCacheKey` and `NarrowTypeCacheKey` hashing byte-identical.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct IndexAccessOptions {
    no_unchecked_indexed_access: bool,
    exact_optional_property_types: bool,
}

solver_options!(IndexAccessOptions {
    no_unchecked_indexed_access / with_no_unchecked_indexed_access,
    exact_optional_property_types / with_exact_optional_property_types,
});
