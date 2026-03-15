//! Type construction boundary helpers.
//!
//! Provides mediated access to solver type construction facilities.
//! Production checker code should prefer purpose-specific helpers here
//! over direct `TypeInterner` access. Test code may use the re-exported
//! `TypeInterner` type for scaffolding.

/// Re-export of `tsz_solver::TypeInterner` for test scaffolding.
///
/// Production checker code should NOT use `TypeInterner` directly.
/// Instead, use the purpose-specific construction helpers in this module
/// or the `TypeDatabase` trait methods. This re-export exists so that
/// checker test code can construct an interner without a direct solver import.
#[allow(unused_imports)]
pub(crate) use tsz_solver::TypeInterner;
