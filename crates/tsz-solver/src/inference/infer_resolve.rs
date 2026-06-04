use crate::inference::infer::{
    InferenceCandidate, InferenceContext, InferenceError, InferenceInfo, InferenceVar,
    MAX_CONSTRAINT_ITERATIONS, MAX_TYPE_RECURSION_DEPTH,
};

use crate::instantiation::instantiate::{TypeSubstitution, instantiate_type};

use crate::operations::widening;

use crate::types::{InferencePriority, ObjectFlags, TemplateSpan, TypeData, TypeId};

use crate::visitor::is_literal_type;

use rustc_hash::FxHashSet;

use tsz_common::interner::Atom;

include!("infer_resolve_parts/part1.rs");
include!("infer_resolve_parts/part2.rs");
