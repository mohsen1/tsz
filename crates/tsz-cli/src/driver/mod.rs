mod core;
pub mod emit;
pub mod resolution;

pub use self::core::*;
// Re-export pub(crate) items from core for crate-internal consumers.
pub(crate) use self::core::CompilationCache;
pub(crate) use self::core::compile_with_cache;
pub(crate) use self::core::compile_with_cache_and_changes;
pub(crate) use self::core::config_base_dir;
pub(crate) use self::core::load_config;
pub(crate) use self::core::normalize_output_dir;
pub(crate) use self::core::resolve_tsconfig_path;
#[cfg(test)]
pub(crate) use self::core::with_types_versions_env;

// Registered here rather than in `core.rs` to keep that monolith under its
// size ratchet; `super::compile` still resolves via the `pub use self::core::*`
// re-export above.
#[cfg(test)]
#[path = "cross_file_user_interface_name_override_tests.rs"]
mod cross_file_user_interface_name_override_tests;

#[cfg(test)]
#[path = "declare_global_interface_keyof_merge_tests.rs"]
mod declare_global_interface_keyof_merge_tests;

#[cfg(test)]
#[path = "cross_file_type_only_namespace_unique_symbol_tests.rs"]
mod cross_file_type_only_namespace_unique_symbol_tests;

#[cfg(test)]
#[path = "nested_homomorphic_mapped_identity_tests.rs"]
mod nested_homomorphic_mapped_identity_tests;
