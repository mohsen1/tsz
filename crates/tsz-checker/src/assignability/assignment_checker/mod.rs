//! Assignment expression checking (simple, compound, logical, readonly).

mod arithmetic_ops;
mod assignment_ops;
mod commonjs_assignment;
mod destructuring;

#[cfg(test)]
#[path = "../assignment_checker_tests.rs"]
mod tests;
