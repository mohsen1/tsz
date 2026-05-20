//! Assignment expression checking (simple, compound, logical, readonly).

mod arithmetic_ops;
mod assignment_ops;
mod commonjs_assignment;
mod destructuring;
mod js_constructor_provisional;
mod js_global_fallback;

#[cfg(test)]
#[path = "../assignment_checker_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../assignment_checker_lib_identity_tests.rs"]
mod lib_identity_tests;
