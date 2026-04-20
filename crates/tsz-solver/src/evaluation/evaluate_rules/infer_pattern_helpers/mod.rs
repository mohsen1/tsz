//! Type-specific infer pattern matching helpers.
//!
//! Contains specialized pattern matchers for different type structures:
//! - Function type patterns
//! - Constructor type patterns
//! - Callable type patterns
//! - Object type patterns
//! - Object with index patterns
//! - Union type patterns
//! - Template literal patterns

use crate::relations::subtype::{SubtypeChecker, TypeResolver};
use crate::types::{
    CallableShapeId, FunctionShape, FunctionShapeId, ObjectShapeId, ParamInfo, TemplateSpan,
    TupleElement, TypeData, TypeId, TypeListId, TypeParamInfo,
};
use crate::utils;
use crate::{TypeSubstitution, instantiate_type};
use rustc_hash::{FxHashMap, FxHashSet};
use tsz_common::interner::Atom;

use super::super::evaluate::TypeEvaluator;

mod constructor_callable;
mod function;
mod object;
mod shared;
mod template;
mod union;
