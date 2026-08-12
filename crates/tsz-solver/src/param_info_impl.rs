//! Inherent constructors and predicates for [`ParamInfo`].
//!
//! Split out of `types.rs` to keep that module under the per-file size cap;
//! the struct definition itself still lives there.

use crate::types::{ParamInfo, TypeId};
use tsz_common::interner::Atom;

impl ParamInfo {
    /// Returns `true` if this parameter is required (non-optional, non-rest).
    pub const fn is_required(&self) -> bool {
        !self.optional && !self.rest
    }

    /// Whether the printer renders this parameter's optional marker (`?`) and
    /// `| undefined` surface. Only display consults this; arity/subtyping read
    /// `optional` directly, so a JS-implicit arity-lenient param prints required.
    pub const fn displays_optional(&self) -> bool {
        self.optional && !self.suppress_display_optional
    }

    /// Create a required parameter.
    pub const fn required(name: Atom, type_id: TypeId) -> Self {
        Self {
            name: Some(name),
            type_id,
            optional: false,
            rest: false,
            suppress_display_optional: false,
        }
    }

    /// Create an optional parameter.
    pub const fn optional(name: Atom, type_id: TypeId) -> Self {
        Self {
            optional: true,
            ..Self::required(name, type_id)
        }
    }

    /// Create a rest parameter.
    pub const fn rest(name: Atom, type_id: TypeId) -> Self {
        Self {
            rest: true,
            ..Self::required(name, type_id)
        }
    }

    /// Create an unnamed required parameter.
    pub const fn unnamed(type_id: TypeId) -> Self {
        Self {
            name: None,
            type_id,
            optional: false,
            rest: false,
            suppress_display_optional: false,
        }
    }
}
