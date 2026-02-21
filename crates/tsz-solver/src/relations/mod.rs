pub(crate) mod compat;
pub(crate) mod judge;
pub(crate) mod lawyer;
pub(crate) mod relation_queries;
pub(crate) mod subtype;
pub(crate) mod subtype_explain;
pub(crate) mod subtype_helpers;
pub(crate) mod subtype_overlap;
pub(crate) mod subtype_rules;
pub(crate) mod subtype_visitor;

pub(crate) use crate::diagnostics::SubtypeFailureReason;
pub(crate) use subtype::{SubtypeChecker, SubtypeResult, TypeResolver};
