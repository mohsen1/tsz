//! The resolved `moduleDetection` compiler setting.
//!
//! This lives in `tsz-common` because module-ness is decided by the binder
//! (`BinderState::is_external_module`) but resolved from `compilerOptions` by
//! the config layer, and consumed by the checker and the emitter. Keeping the
//! kind in one crate lets every consumer name the same value instead of
//! re-deriving it from a pair of booleans.

/// How a source file's module-ness is decided.
///
/// Mirrors `tsc`'s `ModuleDetectionKind` and the `getSetExternalModuleIndicator`
/// dispatch it drives.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum ModuleDetectionKind {
    /// A file is a module when it carries module syntax, when a `react-jsx`
    /// file uses a JSX tag, or when its extension forces a module format
    /// (`.mts`/`.cts`/`.mjs`/`.cjs`, non-declaration files only).
    #[default]
    Auto,
    /// A file is a module only when it carries module syntax — an
    /// import/export declaration, an exported declaration, `export =`, or
    /// `import.meta`. Extensions never force module-ness.
    Legacy,
    /// Every non-declaration file is a module. Declaration files still require
    /// module syntax.
    Force,
}

impl ModuleDetectionKind {
    /// Parse the `compilerOptions.moduleDetection` string, case-insensitively.
    ///
    /// Returns `None` for an unrecognized value so the caller can keep its own
    /// default (`tsc` reports an option diagnostic and falls back separately).
    #[must_use]
    pub const fn from_option_str(value: &str) -> Option<Self> {
        if value.eq_ignore_ascii_case("force") {
            Some(Self::Force)
        } else if value.eq_ignore_ascii_case("legacy") {
            Some(Self::Legacy)
        } else if value.eq_ignore_ascii_case("auto") {
            Some(Self::Auto)
        } else {
            None
        }
    }
}
