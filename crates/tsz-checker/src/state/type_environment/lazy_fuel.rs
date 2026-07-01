//! Retired module for checker lazy-resolution fuel bookkeeping.
//!
//! The fuel counter now lives on `tsz_solver::evaluation::session::EvaluationSession`,
//! which is already shared across parent and child checker contexts.
