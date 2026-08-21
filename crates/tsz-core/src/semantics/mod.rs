//! Single-universe semantic engine.

mod checker;
mod relation;
mod types;

pub(crate) use checker::{CheckResult, check_program};
