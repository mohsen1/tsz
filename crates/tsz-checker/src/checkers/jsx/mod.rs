//! JSX type-checking subsystem.
//!
//! Split by responsibility:
//! - `orchestration`: main entry points, element resolution, namespace lookups
//! - `children`: child normalization, shape validation, contextual typing
//! - `props`: attribute checking, props extraction, overload/spread/union
//! - `runtime`: factory scope, import source, fragment factory
//! - `diagnostics`: display target building, error message rendering

mod children;
mod diagnostics;
mod orchestration;
mod props;
pub(crate) mod runtime;

#[cfg(test)]
mod tests;
