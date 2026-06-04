mod display_order;

mod key_types;

mod keyof_constraint;

use crate::construction::TypeDatabase;

use crate::instantiation::instantiate::{
    TypeSubstitution, instantiate_type, instantiate_type_preserving,
    instantiate_type_preserving_with_declared,
};

use crate::objects::PropertyCollectionResult;

use crate::relations::subtype::{SubtypeChecker, TypeResolver};

use crate::types::Visibility;

use crate::types::{
    IndexSignature, IntrinsicKind, LiteralValue, MappedModifier, MappedType, ObjectFlags,
    ObjectShape, PropertyInfo, TypeData, TypeId,
};

use crate::visitor::keyof_inner_type;

use rustc_hash::{FxHashMap, FxHashSet};

use tsz_common::interner::Atom;

use super::super::evaluate::TypeEvaluator;

#[cfg(test)]
mod mapped_tests;

#[cfg(test)]
mod tests;

pub(crate) use key_types::{MappedKey, MappedKeys};

include!("mapped_parts/part1.rs");
include!("mapped_parts/part2.rs");
