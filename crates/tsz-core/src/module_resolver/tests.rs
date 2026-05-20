//! Module resolver test suite, split by resolver behavior.
//!
//! Each submodule groups tests by a single facet of resolution so that
//! reviewers can locate "what TypeScript rule does this protect?" without
//! reading the entire suite. See `tests::<name>` module docs for the
//! contract each file covers.

mod fixtures;

mod cache_statistics;
mod diagnostics_ts2307;
mod diagnostics_ts2792;
mod diagnostics_ts2835;
mod lookup_classify;
mod lookup_integration;
mod module_extension;
mod node16_modes;
mod package_exports_imports;
mod package_json_data;
mod pattern_matching;
mod resolution_failure;
mod resolver_integration;
mod specifier_parsing;
