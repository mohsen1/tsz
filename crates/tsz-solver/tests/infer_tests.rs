use super::*;
use crate::construction::TypeInterner;
use crate::def::DefId;
use crate::inference::infer::{InferenceContext, InferenceError};
use crate::relations::compat::CompatChecker;
use crate::types::LiteralValue;
use crate::{AssignabilityChecker, ConditionalType, infer_generic_function};

#[path = "infer_tests/advanced_patterns.rs"]
mod advanced_patterns;
#[path = "infer_tests/basics.rs"]
mod basics;
#[path = "infer_tests/bct_and_context.rs"]
mod bct_and_context;
#[path = "infer_tests/bounds_core.rs"]
mod bounds_core;
#[path = "infer_tests/bounds_number_index_a.rs"]
mod bounds_number_index_a;
#[path = "infer_tests/bounds_number_index_b.rs"]
mod bounds_number_index_b;
#[path = "infer_tests/bounds_shapes.rs"]
mod bounds_shapes;
#[path = "infer_tests/constructors_methods_aliases.rs"]
mod constructors_methods_aliases;
#[path = "infer_tests/context_overloads_solv16.rs"]
mod context_overloads_solv16;
#[path = "infer_tests/higher_order_constraints.rs"]
mod higher_order_constraints;
#[path = "infer_tests/literal_widening_and_union.rs"]
mod literal_widening_and_union;
#[path = "infer_tests/recursive_and_params.rs"]
mod recursive_and_params;
#[path = "infer_tests/template_literals.rs"]
mod template_literals;
#[path = "infer_tests/tuples_and_narrowing.rs"]
mod tuples_and_narrowing;
