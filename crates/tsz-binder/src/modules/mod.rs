//! Module, namespace, and import/export binding.
//!
//! Groups all module-related binder logic:
//! - `binding` — module/namespace declaration binding, augmentation, export population
//! - `import_export` — import/export declaration binding and symbol resolution
//! - `resolution_debug` — debugging infrastructure for module resolution

mod binding;
mod import_export;
pub(crate) mod resolution_debug;
