//! Assignment expression checking (simple, compound, logical, readonly).

mod arithmetic_ops;
mod assignment_declared_types;
mod assignment_ops;
mod commonjs_assignment;
mod destructuring;
mod js_constructor_provisional;
mod js_global_fallback;
mod optional_chain_target;
mod polymorphic_this_display;
mod rhs_literal_walker;

#[cfg(test)]
#[path = "../assignment_checker_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../assignment_checker_generic_display_tests.rs"]
mod generic_display_tests;

#[cfg(test)]
#[path = "../assignment_checker_lib_identity_tests.rs"]
mod lib_identity_tests;
