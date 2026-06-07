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
