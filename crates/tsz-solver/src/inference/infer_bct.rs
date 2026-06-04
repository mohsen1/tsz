use crate::operations::expression_ops::normalize_fresh_object_literal_union_members;

use crate::types::{
    CallSignature, CallableShape, FunctionShape, IntrinsicKind, LiteralValue, ObjectShape,
    ObjectShapeId, ParamInfo, PropertyInfo, TupleElement, TypeData, TypeId,
};

use crate::utils::{self, TupleRestExpansion};

use crate::visitor;

use rustc_hash::FxHashSet;

use tsz_common::interner::Atom;

use super::InferenceContext;

include!("infer_bct_parts/part1.rs");
include!("infer_bct_parts/part2.rs");
