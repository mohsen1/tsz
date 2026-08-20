//! Single-universe semantic engine.

mod checker;
mod relation;
mod types;

pub use checker::{CheckResult, check_program};
pub use relation::{RelationFailure, RelationFailureKind, RelationMode};
pub use types::{Completion, DeferredType, Property, Signature, TypeId, TypeKind, TypeStore};
