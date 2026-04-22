//! Prototype-only sound-checking helpers used by tests.
//!
//! This module is intentionally `#[cfg(test)]` and is NOT wired into production
//! checker/CLI/LSP code paths. Keep names, docs, and behavior here scoped to
//! experimentation so they do not imply shipped Sound Mode semantics.

use crate::TypeDatabase;
use crate::judge::JudgeConfig;
use crate::relations::subtype::{SubtypeChecker, TypeEnvironment};
use crate::types::{TypeData, TypeId};

// =============================================================================
// Sound Mode Diagnostics
// =============================================================================

/// Sound Mode diagnostic codes.
///
/// These use the `TS9xxx` range to distinguish from standard TypeScript errors.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum SoundDiagnosticCode {
    /// TS9001: Excess property via sticky freshness.
    /// Object literal has excess properties that would be lost.
    ExcessPropertyStickyFreshness = 9001,

    /// TS9002: Mutable array covariance.
    /// Assigning Dog[] to Animal[] allows pushing Cat.
    MutableArrayCovariance = 9002,

    /// TS9003: Method bivariance.
    /// Method parameters should be contravariant, not bivariant.
    MethodBivariance = 9003,

    /// TS9004: Any escape.
    /// `any` is being used to bypass structural checks.
    AnyEscape = 9004,

    /// TS9005: Enum-number assignment.
    /// Enum values should not be freely assignable to/from number.
    EnumNumberAssignment = 9005,
}

impl SoundDiagnosticCode {
    /// Get the numeric code.
    pub const fn code(self) -> u32 {
        self as u32
    }

    /// Get the diagnostic message template.
    pub const fn message(self) -> &'static str {
        match self {
            Self::ExcessPropertyStickyFreshness => {
                "Object literal has excess property '{0}' which will be silently lost when assigned to type '{1}'."
            }
            Self::MutableArrayCovariance => {
                "Type '{0}[]' is not safely assignable to type '{1}[]'. Array is mutable and may receive incompatible elements."
            }
            Self::MethodBivariance => {
                "Method parameter type '{0}' is not contravariant with '{1}'. Methods should use strict parameter checking."
            }
            Self::AnyEscape => {
                "Type 'any' is being used to bypass type checking. Consider using a more specific type or 'unknown'."
            }
            Self::EnumNumberAssignment => {
                "Enum '{0}' should not be assigned to/from number without explicit conversion."
            }
        }
    }
}

/// A diagnostic emitted by Sound Mode checking.
#[derive(Clone, Debug)]
pub struct SoundDiagnostic {
    /// The diagnostic code
    pub code: SoundDiagnosticCode,

    /// Message arguments for formatting
    pub args: Vec<String>,
}

impl SoundDiagnostic {
    /// Create a new Sound Mode diagnostic.
    pub const fn new(code: SoundDiagnosticCode) -> Self {
        Self {
            code,
            args: Vec::new(),
        }
    }

    /// Add a message argument.
    pub fn with_arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    /// Format the diagnostic message.
    pub fn format_message(&self) -> String {
        let mut msg = self.code.message().to_string();
        for (i, arg) in self.args.iter().enumerate() {
            let placeholder = format!("{{{i}}}");
            msg = msg.replace(&placeholder, arg);
        }
        msg
    }
}

// =============================================================================
// Sound Lawyer
// =============================================================================

/// The "Sound Lawyer" - strict type checking that bypasses TypeScript quirks.
///
/// While the regular `CompatChecker` (Lawyer) applies TypeScript's unsound rules
/// for compatibility, the Sound Lawyer enforces proper type theory semantics:
///
/// - Function parameters are contravariant (not bivariant)
/// - Arrays are invariant for mutation (not covariant)
/// - `any` is only a top type (not also a bottom type)
/// - Enums are distinct from numbers
///
/// ## Usage
///
/// ```text
/// let sound_lawyer = SoundLawyer::new(&interner, &env, config);
///
/// // Strict assignability check
/// let result = sound_lawyer.is_assignable(source, target);
///
/// // Check with diagnostic collection
/// let mut diagnostics = vec![];
/// sound_lawyer.check_assignment(source, target, &mut diagnostics);
/// ```
pub struct SoundLawyer<'a> {
    db: &'a dyn TypeDatabase,
    env: &'a TypeEnvironment,
    config: JudgeConfig,
}

impl<'a> SoundLawyer<'a> {
    /// Create a new Sound Lawyer.
    pub fn new(db: &'a dyn TypeDatabase, env: &'a TypeEnvironment, config: JudgeConfig) -> Self {
        SoundLawyer { db, env, config }
    }

    /// Check if source is assignable to target under sound typing rules.
    pub fn is_assignable(&mut self, source: TypeId, target: TypeId) -> bool {
        // Fast paths
        if source == target {
            return true;
        }
        if target == TypeId::UNKNOWN {
            return true;
        }
        if source == TypeId::NEVER {
            return true;
        }

        // In sound mode, any is ONLY a top type, not a bottom type
        // any is assignable TO everything, but only any/unknown are assignable FROM any
        if target == TypeId::ANY {
            return true;
        }
        if source.is_any() {
            // In sound mode, any can only be assigned to any or unknown
            return target.is_any_or_unknown();
        }

        // Error types
        if source.is_error() || target.is_error() {
            return source == target;
        }

        // Use SubtypeChecker with strict settings
        let mut checker = SubtypeChecker::with_resolver(self.db, self.env);
        checker.strict_function_types = true; // Always contravariant
        checker.allow_void_return = false; // Strict void handling
        checker.allow_bivariant_rest = false; // No bivariant rest params
        checker.disable_method_bivariance = true; // Methods are also contravariant
        checker.strict_null_checks = self.config.strict_null_checks;
        checker.exact_optional_property_types = self.config.exact_optional_property_types;
        checker.no_unchecked_indexed_access = self.config.no_unchecked_indexed_access;

        checker.is_subtype_of(source, target)
    }

    /// Check for unsafe mutable array covariance.
    fn check_array_covariance(&self, source: TypeId, target: TypeId) -> Option<SoundDiagnostic> {
        let source_key = self.db.lookup(source)?;
        let target_key = self.db.lookup(target)?;

        // Check for Array<S> -> Array<T> where S <: T but S != T
        if let (TypeData::Array(s_elem), TypeData::Array(t_elem)) = (&source_key, &target_key)
            && s_elem != t_elem
        {
            // Different element types - this is potentially unsafe covariance
            let mut checker = SubtypeChecker::with_resolver(self.db, self.env);
            checker.strict_function_types = true;

            // Only flag if S <: T (covariant direction)
            // If neither is subtype, it's already an error
            if checker.is_subtype_of(*s_elem, *t_elem) && !checker.is_subtype_of(*t_elem, *s_elem) {
                return Some(
                    SoundDiagnostic::new(SoundDiagnosticCode::MutableArrayCovariance)
                        .with_arg(format!("{s_elem:?}"))
                        .with_arg(format!("{t_elem:?}")),
                );
            }
        }

        None
    }

    // Sticky freshness handling lives in the checker-side SoundFlowAnalyzer.
}

// =============================================================================
// Sound Mode Configuration
// =============================================================================

/// Configuration for Sound Mode checking.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SoundModeConfig {
    /// Enable sticky freshness for excess property checking.
    pub sticky_freshness: bool,

    /// Disallow any as a bottom type (any -> T).
    pub strict_any: bool,

    /// Require arrays to be invariant.
    pub strict_array_covariance: bool,

    /// Require method parameters to be contravariant.
    pub strict_method_bivariance: bool,

    /// Require explicit enum-to-number conversion.
    pub strict_enums: bool,
}

impl Default for SoundModeConfig {
    fn default() -> Self {
        Self {
            sticky_freshness: true,
            strict_any: true,
            strict_array_covariance: true,
            strict_method_bivariance: true,
            strict_enums: true,
        }
    }
}

impl SoundModeConfig {
    /// Create a configuration with all sound checks enabled.
    pub fn all() -> Self {
        Self::default()
    }

    /// Create a minimal configuration (for gradual adoption).
    pub const fn minimal() -> Self {
        Self {
            sticky_freshness: true,
            strict_any: false,
            strict_array_covariance: false,
            strict_method_bivariance: false,
            strict_enums: false,
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
#[path = "../tests/sound_tests.rs"]
mod tests;
