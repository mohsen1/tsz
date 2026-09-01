//! Module resolver test suite, split by resolver behavior.
//!
//! Each submodule groups tests by a single facet of resolution so that
//! reviewers can locate "what TypeScript rule does this protect?" without
//! reading the entire suite. See `tests::<name>` module docs for the
//! contract each file covers.

mod fixtures;

mod arbitrary_extension_ts6263_family;
mod cache_statistics;
mod canonical_entry_path;
mod conditional_types_flavor;
mod diagnostics_ts2307;
mod diagnostics_ts2792;
mod diagnostics_ts2835;
mod explicit_root_untyped_js;
mod importing_module_kind;
mod json_decl_companion;
mod lookup_classify;
mod lookup_integration;
mod max_node_module_js_depth;
mod mixed_esm_cjs_exports;
mod module_extension;
mod node16_modes;
mod node_protocol_builtins;
mod null_target_blocking;
mod package_exports_imports;
mod package_json_data;
mod path_existence_reset;
mod path_mapping;
mod pattern_matching;
mod resolution_failure;
mod resolver_integration;
mod specifier_parsing;
mod target_package_type;
