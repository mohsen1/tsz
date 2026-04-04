//! Assignment expression checking (simple, compound, logical, readonly).

mod assignment_ops;
mod destructuring;

#[cfg(test)]
#[path = "../assignment_checker_tests.rs"]
mod tests;
