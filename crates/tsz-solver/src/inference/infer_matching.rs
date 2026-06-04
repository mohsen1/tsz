use crate::def::DefId;

use crate::instantiation::instantiate::{TypeSubstitution, instantiate_generic, instantiate_type};

use crate::relations::variance::compute_type_param_variances_with_resolver;

use crate::types::{
    CallSignature, CallableShapeId, FunctionShape, FunctionShapeId, InferencePriority,
    IntrinsicKind, LiteralValue, MappedTypeId, ObjectShapeId, ParamInfo, PropertyInfo,
    TemplateLiteralId, TemplateSpan, TupleElement, TupleListId, TypeApplicationId, TypeData,
    TypeId, TypeListId, Variance,
};

use rustc_hash::FxHashMap;

use tsz_common::interner::Atom;

use super::infer::{InferenceContext, InferenceError, InferenceVar};

use super::template_anchor::{find_leftmost_occurrence, find_next_anchor_alternatives};

use super::template_segment_prefix::match_template_segment_prefix;

include!("infer_matching_parts/part1.rs");
include!("infer_matching_parts/part2.rs");
