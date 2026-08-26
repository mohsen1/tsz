//! Single-universe semantic engine.

macro_rules! completed {
    ($value:expr) => {
        match $value {
            Completion::Complete(value) => value,
            Completion::Deferred => return Completion::Deferred,
            Completion::Cycle => return Completion::Cycle,
            Completion::Limit => return Completion::Limit,
        }
    };
}

// Type forms without recursively owned children. Error and Invalid stay
// operation-local because their completion policy differs by caller.
macro_rules! non_recursive_type_kind {
    () => {
        TypeKind::Any
            | TypeKind::Unknown
            | TypeKind::Never
            | TypeKind::Void
            | TypeKind::Undefined
            | TypeKind::Null
            | TypeKind::Boolean
            | TypeKind::Number
            | TypeKind::String
            | TypeKind::BigInt
            | TypeKind::ObjectKeyword
            | TypeKind::Symbol
            | TypeKind::LiteralBoolean(_, _)
            | TypeKind::LiteralNumber(_, _)
            | TypeKind::LiteralString(_, _)
            | TypeKind::TypeParameter { .. }
            | TypeKind::ClassConstructor { .. }
    };
}

mod checker;
mod relation;
mod types;

pub(crate) use checker::{
    CheckResult, DeclarationDisplayParts, DeclarationDisplaySummaries, DeclarationDisplaySummary,
    check_program, summarize_program,
};
