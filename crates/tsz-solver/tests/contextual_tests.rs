use super::*;
use crate::construction::TypeInterner;
use crate::inference::infer::InferenceContext;
use crate::relations::compat::CompatChecker;
use crate::{TupleElement, infer_generic_function};
include!("contextual_tests_parts/part_00.rs");
include!("contextual_tests_parts/part_01.rs");
