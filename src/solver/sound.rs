//! Sound Mode: Strict type checking beyond TypeScript's defaults.
//!
//! TypeScript's type system has intentional unsoundness for pragmatic reasons.
//! Sound Mode provides opt-in stricter checking that catches common bugs.
//!
//! ## Activation
//!
//! - CLI: `tsz check --sound`
//! - tsconfig.json: `{ "compilerOptions": { "sound": true } }`
//! - Per-file pragma: `// @ts-sound`
//!
//! ## What Sound Mode Catches
//!
//! | Issue | TypeScript | Sound Mode |
//! |-------|-----------|------------|
//! | Covariant mutable arrays | ✅ Allowed | ❌ TS9002 |
//! | Method parameter bivariance | ✅ Allowed | ❌ TS9003 |
//! | `any` escapes | ✅ Allowed | ❌ TS9004 |
//! | Excess property bypass | ✅ Allowed | ❌ TS9001 |
//! | Enum-number assignment | ✅ Allowed | ❌ TS9005 |
//!
//! ## Sticky Freshness
//!
//! TypeScript's excess property checking has a bypass:
//!
//! ```typescript
//! const point3d = { x: 1, y: 2, z: 3 };
//! const point2d: { x: number; y: number } = point3d; // ✅ No error!
//! ```
//!
//! Sound Mode introduces "Sticky Freshness" - object literals remain subject
//! to excess property checks as long as they flow through inferred types.
//!
//! See `docs/architecture/SOLVER_REFACTORING_PROPOSAL.md` Section 1.3.1

use crate::solver::TypeDatabase;
use crate::solver::judge::JudgeConfig;
use crate::solver::subtype::{SubtypeChecker, TypeEnvironment};
use crate::solver::types::*;

// =============================================================================
// Sound Mode Diagnostics
// =============================================================================

/// Sound Mode diagnostic codes.
///
/// These use the TS9xxx range to distinguish from standard TypeScript errors.
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

    /// TS9006: Missing index signature.
    /// Object being used as a map without proper index signature.
    MissingIndexSignature = 9006,

    /// TS9007: Unsafe type assertion.
    /// Type assertion doesn't match actual runtime type.
    UnsafeTypeAssertion = 9007,

    /// TS9008: Unchecked indexed access.
    /// Accessing array/object by index without undefined check.
    UncheckedIndexedAccess = 9008,
}

impl SoundDiagnosticCode {
    /// Get the numeric code.
    pub fn code(self) -> u32 {
        self as u32
    }

    /// Get the diagnostic message template.
    pub fn message(self) -> &'static str {
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
            Self::MissingIndexSignature => {
                "Type '{0}' is being used as a map but lacks an index signature. Add '[key: string]: {1}' to the type."
            }
            Self::UnsafeTypeAssertion => {
                "Type assertion from '{0}' to '{1}' may be unsafe. The types do not overlap sufficiently."
            }
            Self::UncheckedIndexedAccess => {
                "Indexed access '{0}[{1}]' may return undefined. Add a null check or enable noUncheckedIndexedAccess."
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

    /// Source location (file_id, start, end)
    pub location: Option<(u32, u32, u32)>,
}

impl SoundDiagnostic {
    /// Create a new Sound Mode diagnostic.
    pub fn new(code: SoundDiagnosticCode) -> Self {
        SoundDiagnostic {
            code,
            args: Vec::new(),
            location: None,
        }
    }

    /// Add a message argument.
    pub fn with_arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    /// Set the source location.
    pub fn with_location(mut self, file_id: u32, start: u32, end: u32) -> Self {
        self.location = Some((file_id, start, end));
        self
    }

    /// Format the diagnostic message.
    pub fn format_message(&self) -> String {
        let mut msg = self.code.message().to_string();
        for (i, arg) in self.args.iter().enumerate() {
            let placeholder = format!("{{{}}}", i);
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
/// ```ignore
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
        if source == TypeId::ANY {
            // In sound mode, any can only be assigned to any or unknown
            return target == TypeId::ANY || target == TypeId::UNKNOWN;
        }

        // Error types
        if source == TypeId::ERROR || target == TypeId::ERROR {
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

    /// Check assignment and collect diagnostics.
    pub fn check_assignment(
        &mut self,
        source: TypeId,
        target: TypeId,
        diagnostics: &mut Vec<SoundDiagnostic>,
    ) -> bool {
        // Check for any escape
        if self.is_any_escape(source, target) {
            diagnostics.push(SoundDiagnostic::new(SoundDiagnosticCode::AnyEscape));
            return false;
        }

        // Check for mutable array covariance
        if let Some(diag) = self.check_array_covariance(source, target) {
            diagnostics.push(diag);
            return false;
        }

        // Standard assignability
        self.is_assignable(source, target)
    }

    /// Check for "any escape" - using any to bypass type checks.
    fn is_any_escape(&self, source: TypeId, target: TypeId) -> bool {
        // any escaping to a non-top type
        source == TypeId::ANY && target != TypeId::ANY && target != TypeId::UNKNOWN
    }

    /// Check for unsafe mutable array covariance.
    fn check_array_covariance(&self, source: TypeId, target: TypeId) -> Option<SoundDiagnostic> {
        let source_key = self.db.lookup(source)?;
        let target_key = self.db.lookup(target)?;

        // Check for Array<S> -> Array<T> where S <: T but S != T
        if let (TypeKey::Array(s_elem), TypeKey::Array(t_elem)) = (&source_key, &target_key) {
            if s_elem != t_elem {
                // Different element types - this is potentially unsafe covariance
                let mut checker = SubtypeChecker::with_resolver(self.db, self.env);
                checker.strict_function_types = true;

                // Only flag if S <: T (covariant direction)
                // If neither is subtype, it's already an error
                if checker.is_subtype_of(*s_elem, *t_elem)
                    && !checker.is_subtype_of(*t_elem, *s_elem)
                {
                    return Some(
                        SoundDiagnostic::new(SoundDiagnosticCode::MutableArrayCovariance)
                            .with_arg(format!("{:?}", s_elem))
                            .with_arg(format!("{:?}", t_elem)),
                    );
                }
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
        SoundModeConfig {
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
        SoundModeConfig::default()
    }

    /// Create a minimal configuration (for gradual adoption).
    pub fn minimal() -> Self {
        SoundModeConfig {
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
mod tests {
    use super::*;
    use crate::solver::TypeInterner;

    fn create_test_interner() -> TypeInterner {
        TypeInterner::new()
    }

    #[test]
    fn test_sound_diagnostic_formatting() {
        let diag = SoundDiagnostic::new(SoundDiagnosticCode::ExcessPropertyStickyFreshness)
            .with_arg("extraProp")
            .with_arg("{ x: number }");

        let msg = diag.format_message();
        assert!(msg.contains("extraProp"));
        assert!(msg.contains("{ x: number }"));
    }

    #[test]
    fn test_sound_lawyer_any_escape() {
        let interner = create_test_interner();
        let env = TypeEnvironment::new();
        let config = JudgeConfig::default();
        let mut lawyer = SoundLawyer::new(&interner, &env, config);

        // In sound mode, any -> number should be flagged
        assert!(!lawyer.is_assignable(TypeId::ANY, TypeId::NUMBER));

        // But number -> any is fine
        assert!(lawyer.is_assignable(TypeId::NUMBER, TypeId::ANY));

        // any -> any is fine
        assert!(lawyer.is_assignable(TypeId::ANY, TypeId::ANY));

        // any -> unknown is fine
        assert!(lawyer.is_assignable(TypeId::ANY, TypeId::UNKNOWN));
    }

    #[test]
    fn test_sound_lawyer_array_covariance() {
        let interner = create_test_interner();
        let env = TypeEnvironment::new();
        let config = JudgeConfig::default();
        let lawyer = SoundLawyer::new(&interner, &env, config);

        // Create Array<number> and Array<string>
        let array_number = interner.array(TypeId::NUMBER);
        let array_string = interner.array(TypeId::STRING);

        // These should fail
        assert!(
            lawyer
                .check_array_covariance(array_number, array_string)
                .is_none()
        );
        assert!(
            lawyer
                .check_array_covariance(array_string, array_number)
                .is_none()
        );

        // Same type is fine
        assert!(
            lawyer
                .check_array_covariance(array_number, array_number)
                .is_none()
        );
    }

    #[test]
    fn test_sound_mode_config() {
        let all = SoundModeConfig::all();
        assert!(all.sticky_freshness);
        assert!(all.strict_any);
        assert!(all.strict_array_covariance);
        assert!(all.strict_method_bivariance);
        assert!(all.strict_enums);

        let minimal = SoundModeConfig::minimal();
        assert!(minimal.sticky_freshness);
        assert!(!minimal.strict_any);
    }

    #[test]
    fn test_sound_diagnostic_codes() {
        assert_eq!(
            SoundDiagnosticCode::ExcessPropertyStickyFreshness.code(),
            9001
        );
        assert_eq!(SoundDiagnosticCode::MutableArrayCovariance.code(), 9002);
        assert_eq!(SoundDiagnosticCode::MethodBivariance.code(), 9003);
        assert_eq!(SoundDiagnosticCode::AnyEscape.code(), 9004);
        assert_eq!(SoundDiagnosticCode::EnumNumberAssignment.code(), 9005);
    }
}
