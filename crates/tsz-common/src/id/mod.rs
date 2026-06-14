//! Declarative generator for `u32` identity newtypes.
//!
//! tsz threads a family of `u32` "handle" newtypes through every layer:
//! interned string atoms, solver `TypeId`/shape ids, binder
//! `SymbolId`/`ScopeId`/`FlowNodeId`, the parser `NodeIndex`, and the core
//! `SourceId`. They all share the same skeleton — a `pub struct N(pub u32)`
//! with the `Copy, Clone, Debug, PartialEq, Eq, Hash` derive cluster — and
//! several share a null/sentinel helper block (`NONE`, `is_none`, …).
//!
//! [`define_id!`] is the single source of truth for that skeleton so the
//! derive surface and the sentinel helpers cannot drift from handle to handle.
//! Type-specific associated items (`TypeId::ERROR`, `DefId::INVALID`,
//! `SourceId::new`, …) stay in adjacent inherent `impl` blocks; the macro only
//! emits the boilerplate shared across handles.

/// Declare a `u32` identity newtype (an interned handle or arena index).
///
/// Every handle gets the base derive cluster
/// `Copy, Clone, Debug, PartialEq, Eq, Hash`. Additional derives — serde,
/// `Default`, `Ord` — are listed after `derive:`. Attributes written before the
/// `struct` keyword (doc comments, `#[wasm_bindgen]`, `cfg_attr`, …) are passed
/// through verbatim, preserving their order relative to the generated
/// `#[derive(...)]`.
///
/// An optional `sentinel:` clause emits the shared null-handle helpers,
/// parameterized by the family's null convention:
///
/// - `sentinel: zero` — `NONE = Self(0)` plus `none()` (a serde-default
///   helper), `is_none()`, and the `index()` accessor (the interned-string
///   atom convention).
/// - `sentinel: max` — `NONE = Self(u32::MAX)` plus `is_none()` / `is_some()`
///   (the binder/parser arena-index convention).
/// - `sentinel: max + into_option` — as `max`, plus `into_option()`.
///
/// # Examples
///
/// ```ignore
/// define_id!(/// A plain interned shape id.
///     pub struct ObjectShapeId);
///
/// define_id!(/// An arena node index.
///     pub struct NodeIndex; derive: Default, Serialize, Deserialize;
///     sentinel: max + into_option);
/// ```
#[macro_export]
macro_rules! define_id {
    // --- With a sentinel helper block (always carries extra derives) ---
    (
        $(#[$meta:meta])*
        $vis:vis struct $name:ident
        ; derive: $($extra:path),+ $(,)?
        ; sentinel: $($sentinel:tt)+
    ) => {
        $crate::define_id!(@emit $(#[$meta])* $vis struct $name : $($extra),+ );
        $crate::define_id!(@sentinel $name; $($sentinel)+);
    };

    // --- Without a sentinel helper block; extra derives optional ---
    (
        $(#[$meta:meta])*
        $vis:vis struct $name:ident
        $(; derive: $($extra:path),+ $(,)?)?
        $(;)?
    ) => {
        $crate::define_id!(@emit $(#[$meta])* $vis struct $name $(: $($extra),+)? );
    };

    // --- Internal: emit the struct with the base + extra derive cluster ---
    (@emit
        $(#[$meta:meta])*
        $vis:vis struct $name:ident $(: $($extra:path),+)?
    ) => {
        $(#[$meta])*
        #[derive(Copy, Clone, Debug, PartialEq, Eq, Hash $($(, $extra)+)?)]
        $vis struct $name(pub u32);
    };

    // --- Internal: zero-valued sentinel (interned-string atom convention) ---
    (@sentinel $name:ident; zero) => {
        impl $name {
            #[doc = concat!(
                "Sentinel value representing no `", stringify!($name),
                "` / the empty atom."
            )]
            pub const NONE: Self = Self(0);

            /// Returns the `NONE` sentinel — used as a serde default.
            #[must_use]
            #[inline]
            pub const fn none() -> Self {
                Self::NONE
            }

            /// Whether this handle is the `NONE` sentinel.
            #[must_use]
            #[inline]
            pub const fn is_none(self) -> bool {
                self.0 == 0
            }

            /// The raw `u32` index value.
            #[must_use]
            #[inline]
            pub const fn index(self) -> u32 {
                self.0
            }
        }
    };

    // --- Internal: max-valued sentinel (binder/parser arena-index convention) ---
    (@sentinel $name:ident; max) => {
        impl $name {
            #[doc = concat!("Sentinel value representing no `", stringify!($name), "`.")]
            pub const NONE: Self = Self(u32::MAX);

            /// Whether this handle is the `NONE` sentinel.
            #[must_use]
            #[inline]
            pub const fn is_none(&self) -> bool {
                self.0 == u32::MAX
            }

            /// Whether this handle is a real (non-`NONE`) handle.
            #[must_use]
            #[inline]
            pub const fn is_some(&self) -> bool {
                self.0 != u32::MAX
            }
        }
    };

    // --- Internal: max-valued sentinel plus `into_option` ---
    (@sentinel $name:ident; max + into_option) => {
        $crate::define_id!(@sentinel $name; max);
        impl $name {
            #[doc = concat!(
                "Convert a sentinel-based optional `", stringify!($name),
                "` into an `Option`."
            )]
            #[must_use]
            #[inline]
            pub const fn into_option(self) -> Option<$name> {
                if self.0 == u32::MAX { None } else { Some(self) }
            }
        }
    };
}
