//! Object property query ecosystem.
//!
//! This module groups solver logic related to object types, property resolution,
//! index signatures, and element access evaluation:
//!
//! - **apparent**: Built-in/intrinsic member resolution for primitives
//! - **collect**: Intersection property collection and merging
//! - **literal**: Object literal type construction builder
//! - **`index_signatures`**: Index signature resolution across type shapes
//! - **`element_access`**: Structured element access evaluation with error classification
pub mod apparent;
mod collect;
pub mod element_access;
pub mod index_signatures;
mod literal;

pub use apparent::{
    ApparentMemberKind, apparent_object_member_kind, apparent_primitive_member_kind,
    apparent_primitive_members,
};
pub use collect::*;
pub use element_access::*;
pub use index_signatures::*;
pub use literal::ObjectLiteralBuilder;
