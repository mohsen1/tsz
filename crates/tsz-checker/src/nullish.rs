//! Nullish Coalescing Type Checking
//!
//! Nullish coalescing (`??`) type computation is handled inline in the
//! checker's expression/assignment paths. Solver-level nullish utilities
//! (e.g. `remove_nullish`, `can_be_nullish`) live in `tsz_solver`.
