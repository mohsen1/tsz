//! Single-universe semantic engine.

macro_rules! completed {
    ($value:expr) => {
        match $value {
            Completion::Complete(value) => value,
            Completion::Deferred => return Completion::Deferred,
            Completion::Cycle => return Completion::Cycle,
            Completion::Limit => return Completion::Limit,
        }
    };
}

mod checker;
mod relation;
mod types;

pub(crate) use checker::{CheckResult, check_program};
