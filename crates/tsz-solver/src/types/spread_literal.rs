//! `ObjectFlags::SPREAD_LITERAL` accessors for `ObjectShape`.
//!
//! A shape produced by an object spread (`{ ...base }`) is anonymous and
//! symbol-less like a hand-written object literal, but `tsc`'s
//! `getSpreadType` never marks its result `ObjectFlags.JSLiteral` the way
//! `createObjectLiteralType` marks a plain literal. The checker's JS
//! "open container" leniency (`js_open_object_receiver_under_implicit_any`,
//! which relaxes an unknown-property access to implicit `any` when
//! `noImplicitAny` is off) must key off that same distinction, so a
//! spread-derived receiver stays a strict `TS2339` target even though a
//! hand-written `{}` container is open. Relations ignore this flag; it is
//! checker-policy-only.

use super::{ObjectFlags, ObjectShape};

impl ObjectShape {
    /// Mark this shape as produced by an object-spread (`{ ...base }`).
    ///
    /// Use this instead of importing `ObjectFlags::SPREAD_LITERAL` directly.
    pub fn mark_spread_literal(&mut self) {
        self.flags |= ObjectFlags::SPREAD_LITERAL;
    }

    /// Return true if this shape was produced by an object-spread.
    pub const fn is_spread_literal(&self) -> bool {
        self.flags.contains(ObjectFlags::SPREAD_LITERAL)
    }
}
