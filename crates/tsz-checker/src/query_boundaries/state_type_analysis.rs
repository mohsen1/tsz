pub(crate) use super::common::{call_signatures_for_type, callable_shape_for_type};

#[cfg(test)]
use tsz_solver::TypeId;

#[cfg(test)]
#[path = "../../tests/state_type_analysis.rs"]
mod tests;
