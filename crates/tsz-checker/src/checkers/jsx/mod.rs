//! JSX type-checking subsystem.
//!
//! Split by responsibility:
//! - `orchestration`: main entry points, element resolution, namespace lookups
//! - `children`: child normalization, shape validation, contextual typing
//! - `extraction`: props extraction from components, component validation
//! - `overloads`: overloaded SFC resolution (TS2769)
//! - `props`: attribute checking, spread/union validation, missing props (TS2741)
//! - `runtime`: factory scope, import source, fragment factory
//! - `diagnostics`: display target building, error message rendering

mod children;
mod diagnostics;
mod extraction;
mod orchestration;
mod overloads;
mod props;
pub(crate) mod runtime;

#[cfg(test)]
mod tests;
