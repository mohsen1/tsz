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
mod spread;

#[cfg(test)]
mod optional_prop_display_tests;
#[cfg(test)]
mod ref_callback_tests;
#[cfg(test)]
mod spread_assignability_tests;
#[cfg(test)]
mod target_display_tests;
#[cfg(test)]
mod tests;
